//! The workspaces on this device: the original one, and one more for each
//! additional cloud account. Every workspace has its own database, receipt
//! files and credential slot, so records, sync work and late replies cannot
//! cross between accounts. Signing in is decided here because it chooses which
//! workspace a session belongs to.
use crate::{
    dispatch::dispatch,
    error::{AppError, Result},
    security::secrets::SecretStore,
    services::{
        cloud_sync::{self, Pkce, Session},
        cloud_transport::{CloudConfig, Transport},
        AppService,
    },
};
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    fs,
    path::PathBuf,
    sync::{Arc, Mutex, RwLock},
    time::{Duration, Instant},
};

/// The original workspace's key; account workspaces are keyed by account ID.
const ORIGINAL: &str = "";

fn fail(message: &str) -> AppError {
    AppError::new("CloudSync", message)
}

pub struct Workspaces {
    base: PathBuf,
    secrets: Arc<dyn SecretStore>,
    cloud: Arc<Transport>,
    /// One service per workspace for the life of the process, so each has a
    /// single database connection, sync worker and session-refresh owner.
    opened: Mutex<HashMap<String, Arc<AppService>>>,
    active: RwLock<(String, Arc<AppService>)>,
    /// The browser sign-in in progress. Its verifier never leaves memory.
    flow: Mutex<Option<Pkce>>,
    /// A session waiting for the person to choose what joins the account.
    waiting: Mutex<Option<(Session, Instant)>>,
}

impl Workspaces {
    pub fn open(base: PathBuf, secrets: Arc<dyn SecretStore>) -> Result<Self> {
        let cloud = Transport::new(CloudConfig::current()?)?;
        Self::open_with_cloud(base, secrets, Arc::new(cloud))
    }
    pub fn open_with_cloud(
        base: PathBuf,
        secrets: Arc<dyn SecretStore>,
        cloud: Arc<Transport>,
    ) -> Result<Self> {
        // The last active account, if its workspace still exists.
        let key = fs::read_to_string(base.join("active-account"))
            .ok()
            .map(|k| k.trim().to_string())
            .filter(|k| {
                uuid::Uuid::parse_str(k).is_ok()
                    && base
                        .join("accounts")
                        .join(k)
                        .join("database/expenses.sqlite")
                        .is_file()
            })
            .unwrap_or_default();
        let root = if key.is_empty() {
            base.clone()
        } else {
            base.join("accounts").join(&key)
        };
        let service = Arc::new(AppService::open_with_cloud(
            root,
            secrets.clone(),
            cloud.clone(),
        )?);
        Ok(Self {
            base,
            secrets,
            cloud,
            opened: Mutex::new(HashMap::from([(key.clone(), service.clone())])),
            active: RwLock::new((key, service)),
            flow: Mutex::default(),
            waiting: Mutex::default(),
        })
    }
    fn current(&self) -> (String, Arc<AppService>) {
        self.active
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }
    /// The workspace every ordinary command runs against.
    pub fn active(&self) -> Arc<AppService> {
        self.current().1
    }
    fn service(&self, key: &str) -> Result<Arc<AppService>> {
        let mut opened = self.opened.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(service) = opened.get(key) {
            return Ok(service.clone());
        }
        let root = if key.is_empty() {
            self.base.clone()
        } else {
            uuid::Uuid::parse_str(key).map_err(|_| fail("Invalid cloud account."))?;
            self.base.join("accounts").join(key)
        };
        let service = Arc::new(AppService::open_with_cloud(
            root,
            self.secrets.clone(),
            self.cloud.clone(),
        )?);
        opened.insert(key.to_string(), service.clone());
        Ok(service)
    }
    /// Makes `key` the workspace commands run against. The one left behind stops
    /// syncing at its next step; anything it still writes lands in its own files.
    fn activate(&self, key: &str) -> Result<Arc<AppService>> {
        let service = self.service(key)?;
        let staging = self.base.join("active-account.part");
        fs::write(&staging, key)?;
        fs::rename(staging, self.base.join("active-account"))?;
        let mut active = self.active.write().unwrap_or_else(|e| e.into_inner());
        if active.0 != key {
            active.1.cloud_cancel();
            *active = (key.to_string(), service.clone());
        }
        Ok(service)
    }
    fn outcome(&self, outcome: &str, service: &AppService) -> Result<Value> {
        Ok(json!({"outcome":outcome,"status":service.cloud_status()?}))
    }
    /// Signs `session` in to the workspace of its own account and makes it active.
    fn enter(&self, session: &Session) -> Result<Value> {
        let user = session.user_id.as_str();
        // The original workspace may already belong to this account.
        let key = if self.service(ORIGINAL)?.cloud_owner()?.as_deref() == Some(user) {
            ORIGINAL
        } else {
            user
        };
        let service = self.service(key)?;
        service.bind_owner(user)?;
        service.accept_session(session)?;
        self.activate(key)?;
        self.outcome("switched", &service)
    }
    /// Decides where a newly verified session belongs. Identity is the account
    /// ID Supabase verified; emails and names are never compared.
    fn route(&self, session: Session) -> Result<Value> {
        let (key, active) = self.current();
        match active.cloud_owner()? {
            Some(owner) if owner == session.user_id => {
                active.accept_session(&session)?;
                self.outcome("signedIn", &active)
            }
            // Another account's workspace is open: this account uses its own.
            Some(_) => self.enter(&session),
            None if !key.is_empty() => Err(fail("This account workspace is incomplete.")),
            None if self.account_exists(&session.user_id) => self.enter(&session),
            None if active.has_user_data()? => {
                // Records already on this device never join an account silently.
                let email = session.email.clone();
                *self.waiting.lock().unwrap_or_else(|e| e.into_inner()) =
                    Some((session, Instant::now()));
                Ok(json!({"outcome":"chooseData","email":email}))
            }
            None => {
                active.bind_owner(&session.user_id)?;
                active.accept_session(&session)?;
                self.outcome("signedIn", &active)
            }
        }
    }
    fn account_exists(&self, user: &str) -> bool {
        self.base
            .join("accounts")
            .join(user)
            .join("database/expenses.sqlite")
            .is_file()
    }
    /// Finishes a first sign-in on a device that already holds records.
    fn adopt(&self, choice: &str) -> Result<Value> {
        let waiting = self
            .waiting
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .take()
            .filter(|(_, since)| since.elapsed() < Duration::from_secs(600));
        let Some((session, _)) = waiting else {
            return Err(fail("That sign-in has expired. Sign in again."));
        };
        match choice {
            "join" => {
                let (key, active) = self.current();
                if !key.is_empty() {
                    return Err(fail(
                        "Only this device's original records can join an account.",
                    ));
                }
                // Both steps are safe to repeat: if this is interrupted, signing in
                // again finds the workspace already linked and simply continues.
                active.bind_owner(&session.user_id)?;
                active.accept_session(&session)?;
                self.outcome("signedIn", &active)
            }
            "separate" => self.enter(&session),
            "cancel" => {
                cloud_sync::revoke(&self.cloud, session.access());
                self.outcome("cancelled", &self.active())
            }
            _ => Err(AppError::invalid("Choose join, separate or cancel.")),
        }
    }
    fn begin(&self, provider: &str, link: bool) -> Result<Value> {
        let (flow, url) = if link {
            let token = self.active().cloud_token()?;
            cloud_sync::link_url(&self.cloud, &token, provider)?
        } else {
            cloud_sync::oauth_url(&self.cloud, provider)?
        };
        // Starting again abandons the earlier attempt; its callback can no longer be redeemed.
        *self.flow.lock().unwrap_or_else(|e| e.into_inner()) = Some(flow);
        Ok(json!({"url":url}))
    }
    fn finish(&self, callback: &str) -> Result<Value> {
        let mut flow = self.flow.lock().unwrap_or_else(|e| e.into_inner());
        let link = flow.as_ref().is_some_and(|f| f.link);
        let session = cloud_sync::oauth_session(&self.cloud, &mut flow, callback)?;
        drop(flow);
        if !link {
            return self.route(session);
        }
        let active = self.active();
        if active.cloud_owner()?.as_deref() != Some(session.user_id.as_str()) {
            cloud_sync::revoke(&self.cloud, session.access());
            return Err(fail(
                "That sign-in belongs to a different account. Nothing was changed.",
            ));
        }
        active.accept_session(&session)?;
        self.outcome("linked", &active)
    }
    /// Every command of the app. Sign-in commands are handled here; the rest
    /// go to whichever workspace is active when they arrive.
    pub fn dispatch(&self, command: &str, a: Value) -> Result<Value> {
        let text = |key: &str| {
            a[key]
                .as_str()
                .ok_or_else(|| AppError::invalid(format!("Missing {key}")))
        };
        match command {
            "workspace_info" => {
                let (key, active) = self.current();
                Ok(json!({"root":active.paths.root,"account":key}))
            }
            "cloud_request_code" => {
                cloud_sync::request_code(&self.cloud, text("email")?)?;
                Ok(serde_json::to_value(self.active().cloud_status()?)?)
            }
            "cloud_verify_code" => self.route(cloud_sync::code_session(
                &self.cloud,
                text("email")?,
                text("code")?,
            )?),
            "cloud_sign_in" => self.route(cloud_sync::password_session(
                &self.cloud,
                text("email")?,
                text("password")?,
            )?),
            "cloud_oauth_start" => {
                self.begin(text("provider")?, a["link"].as_bool().unwrap_or(false))
            }
            "cloud_oauth_finish" => self.finish(text("url")?),
            "cloud_oauth_cancel" => {
                self.flow.lock().unwrap_or_else(|e| e.into_inner()).take();
                Ok(Value::Null)
            }
            "cloud_adopt" => self.adopt(text("choice")?),
            _ => dispatch(&self.active(), command, a),
        }
    }
}
