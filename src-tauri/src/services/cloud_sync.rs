//! Account-bound Supabase synchronization. SQLite remains usable offline.
use super::{
    cloud_transport::{read_limited, Failure, Transport, FILE_LIMIT},
    AppService,
};
use crate::error::{AppError, Result};
use base64::Engine;
use reqwest::Method;
use rusqlite::{params, types::Value as SqlValue, Connection, OptionalExtension};
use serde::Serialize;
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};
use std::{
    fs,
    sync::atomic::Ordering,
    time::{Duration, Instant},
};
use zeroize::Zeroizing;
const TABLES: &[&str] = &[
    "receipt_files",
    "expenses",
    "expense_claims",
    "claim_expenses",
    "extraction_runs",
    "audit_events",
    "exchange_rates",
    "workspaces",
    "categories",
    "accounts",
    "bills",
    "transactions",
    "budgets",
    "budget_alerts",
    "transaction_suggestions",
    "business_profiles",
    "customers",
    "inventory_items",
    "invoices",
    "stock_movements",
    "invoice_payments",
    "receipt_item_reviews",
];
/// Tables whose rows mean this device already holds someone's records.
const USER_DATA: &[&str] = &[
    "receipt_files",
    "expenses",
    "expense_claims",
    "accounts",
    "bills",
    "transactions",
    "budgets",
    "customers",
    "inventory_items",
    "invoices",
];
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CloudStatus {
    signed_in: bool,
    /// A cloud account owns this workspace, whether or not it is signed in now.
    linked: bool,
    email: Option<String>,
    providers: Vec<String>,
    pending: i64,
    conflicts: i64,
    last_sync: Option<String>,
    error: Option<String>,
    /// local | signedOut | signInAgain | offline | needsAttention | pending | upToDate
    state: &'static str,
    failure: Option<String>,
    retry_at: Option<String>,
    request_id: Option<String>,
}
/// A signed-in Supabase session. It never leaves the core.
pub(crate) struct Session {
    access: Zeroizing<String>,
    refresh: Zeroizing<String>,
    expires_in: u64,
    pub(crate) user_id: String,
    pub(crate) email: Option<String>,
    providers: Vec<String>,
}
impl Session {
    fn parse(mut value: Value) -> Result<Self> {
        let mut take = |key: &str| match value[key].take() {
            Value::String(s) if !s.is_empty() => Ok(Zeroizing::new(s)),
            _ => Err(fail("The cloud sign-in response was incomplete.")),
        };
        let (access, refresh) = (take("access_token")?, take("refresh_token")?);
        let user = &value["user"];
        let user_id = user["id"].as_str().unwrap_or_default().to_string();
        uuid::Uuid::parse_str(&user_id)
            .map_err(|_| fail("The cloud sign-in response was incomplete."))?;
        Ok(Self {
            access,
            refresh,
            expires_in: value["expires_in"].as_u64().unwrap_or(0),
            user_id,
            email: user["email"]
                .as_str()
                .filter(|e| !e.is_empty())
                .map(str::to_owned),
            providers: user["app_metadata"]["providers"]
                .as_array()
                .map(|list| {
                    list.iter()
                        .filter_map(|p| p.as_str().map(str::to_owned))
                        .collect()
                })
                .unwrap_or_default(),
        })
    }
    fn cached(&self) -> Access {
        Access {
            token: self.access.clone(),
            until: Instant::now() + Duration::from_secs(self.expires_in.saturating_sub(120)),
        }
    }
    pub(crate) fn access(&self) -> &str {
        &self.access
    }
}
/// The current access token, reused until shortly before it expires.
pub(crate) struct Access {
    token: Zeroizing<String>,
    until: Instant,
}
fn fail(message: &str) -> AppError {
    AppError::new("CloudSync", message)
}
fn cancelled() -> AppError {
    AppError::new(
        "CloudCancelled",
        "Cloud sync stopped because the account changed.",
    )
}
fn token(cloud: &Transport, grant: &str, body: Value) -> Result<Session> {
    Session::parse(
        cloud.json(
            cloud
                .request(Method::POST, &format!("/auth/v1/token?grant_type={grant}"))
                .json(&body),
        )?,
    )
}
pub(crate) fn password_session(cloud: &Transport, email: &str, password: &str) -> Result<Session> {
    token(
        cloud,
        "password",
        json!({"email":email,"password":password}),
    )
}
/// Sends a sign-in email to an existing account. Never creates one.
pub(crate) fn request_code(cloud: &Transport, email: &str) -> Result<()> {
    cloud.send(
        cloud
            .request(Method::POST, "/auth/v1/otp")
            .json(&json!({"email":email,"create_user":false})),
    )?;
    Ok(())
}
/// Accepts a six-digit code, the unopened email link, or the address an opened link landed on.
pub(crate) fn code_session(cloud: &Transport, email: &str, code: &str) -> Result<Session> {
    if let Some(refresh) = redirect_session(code)? {
        return token(
            cloud,
            "refresh_token",
            json!({"refresh_token":refresh.as_str()}),
        );
    }
    let body = if code.starts_with("https://") {
        let u = reqwest::Url::parse(code).map_err(|_| fail("Invalid email link."))?;
        if u.host_str() != Some(cloud.config.host().as_str())
            || u.path() != "/auth/v1/verify"
            || u.port().is_some()
            || !u.username().is_empty()
        {
            return Err(fail("Paste the sign-in link from this Folio project."));
        }
        let token_hash = u
            .query_pairs()
            .find(|(k, _)| k == "token")
            .map(|(_, v)| v.to_string())
            .ok_or_else(|| fail("Email link has no verification token."))?;
        json!({"token_hash":token_hash,"type":"email"})
    } else {
        json!({"email":email,"token":code,"type":"email"})
    };
    Session::parse(cloud.json(cloud.request(Method::POST, "/auth/v1/verify").json(&body))?)
}
/// One browser sign-in in progress. The verifier stays in memory, so only this
/// running app can redeem the code Supabase sends back.
pub(crate) struct Pkce {
    verifier: Zeroizing<String>,
    started: Instant,
    pub(crate) link: bool,
}
impl Pkce {
    fn new(link: bool) -> (Self, String) {
        let verifier = Zeroizing::new(format!(
            "{}{}",
            uuid::Uuid::new_v4().simple(),
            uuid::Uuid::new_v4().simple()
        ));
        let challenge = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .encode(Sha256::digest(verifier.as_bytes()));
        (
            Self {
                verifier,
                started: Instant::now(),
                link,
            },
            challenge,
        )
    }
}
fn provider(name: &str) -> Result<&str> {
    if matches!(name, "google" | "apple") {
        Ok(name)
    } else {
        Err(fail("Choose Google or Apple."))
    }
}
/// The browser sign-in methods this project has switched on. `None` when the
/// project cannot be reached, so callers can still offer every method.
pub(crate) fn enabled_providers(cloud: &Transport) -> Option<Vec<String>> {
    let settings = cloud
        .json(
            cloud
                .request(Method::GET, "/auth/v1/settings")
                .timeout(Duration::from_secs(8)),
        )
        .ok()?;
    Some(
        ["google", "apple"]
            .into_iter()
            .filter(|p| settings["external"][*p] == true)
            .map(str::to_owned)
            .collect(),
    )
}
/// Refuses a provider the project has not switched on, with the reason, instead
/// of sending the person to a browser page that only shows an error.
pub(crate) fn require_provider(cloud: &Transport, name: &str) -> Result<()> {
    provider(name)?;
    match enabled_providers(cloud) {
        Some(on) if !on.iter().any(|p| p == name) => Err(fail(&format!(
            "{} sign-in is not set up for this Folio project yet. Use another sign-in method.",
            if name == "apple" { "Apple" } else { "Google" }
        ))),
        _ => Ok(()),
    }
}
/// The address to open in the system browser for Google or Apple sign-in.
pub(crate) fn oauth_url(cloud: &Transport, name: &str) -> Result<(Pkce, String)> {
    let (flow, challenge) = Pkce::new(false);
    let mut url = reqwest::Url::parse(&format!("{}/auth/v1/authorize", cloud.config.url))
        .map_err(|_| fail("Invalid cloud endpoint."))?;
    url.query_pairs_mut()
        .append_pair("provider", provider(name)?)
        .append_pair("redirect_to", &cloud.config.callback)
        .append_pair("code_challenge", &challenge)
        .append_pair("code_challenge_method", "s256");
    Ok((flow, url.into()))
}
/// Adds Google or Apple to the account that is signed in now. Supabase only
/// issues this address to a valid session, which is the proof of ownership.
pub(crate) fn link_url(cloud: &Transport, access: &str, name: &str) -> Result<(Pkce, String)> {
    let (flow, challenge) = Pkce::new(true);
    let reply = cloud.json(
        cloud
            .request(Method::GET, "/auth/v1/user/identities/authorize")
            .bearer_auth(access)
            .query(&[
                ("provider", provider(name)?),
                ("redirect_to", cloud.config.callback.as_str()),
                ("code_challenge", challenge.as_str()),
                ("code_challenge_method", "s256"),
                ("skip_http_redirect", "true"),
            ]),
    )?;
    let url = reply["url"]
        .as_str()
        .filter(|u| u.starts_with("https://") || u.starts_with(&cloud.config.url))
        .ok_or_else(|| fail("The cloud service did not return a sign-in address."))?;
    Ok((flow, url.to_string()))
}
/// Redeems the address the browser was returned to. `flow` is consumed, so a
/// repeated or stale callback cannot sign anyone in.
pub(crate) fn oauth_session(
    cloud: &Transport,
    flow: &mut Option<Pkce>,
    callback: &str,
) -> Result<Session> {
    let expected = reqwest::Url::parse(&cloud.config.callback)
        .map_err(|_| fail("Invalid sign-in callback."))?;
    let url = reqwest::Url::parse(callback).map_err(|_| fail("Invalid sign-in callback."))?;
    if url.scheme() != expected.scheme()
        || url.host_str() != expected.host_str()
        || url.path() != expected.path()
    {
        return Err(fail("That link is not a Folio sign-in callback."));
    }
    // Only a genuine callback ends the attempt; a stray link leaves it waiting.
    let flow = flow.take();
    // Supabase reports provider errors in the query or the fragment.
    let fragment = url.fragment().unwrap_or_default().to_string();
    let pairs: Vec<(String, String)> = url
        .query_pairs()
        .chain(
            reqwest::Url::parse(&format!("x://x/?{fragment}"))
                .map_err(|_| fail("Invalid sign-in callback."))?
                .query_pairs(),
        )
        .map(|(k, v)| (k.into_owned(), v.into_owned()))
        .collect();
    let find = |key: &str| {
        pairs
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.as_str())
    };
    if let Some(reason) = find("error_description").or_else(|| find("error")) {
        let reason: String = reason.chars().take(200).collect();
        return Err(fail(&format!("Sign-in was not completed: {reason}")));
    }
    let code = find("code").ok_or_else(|| fail("The sign-in callback carried no code."))?;
    let flow = flow.ok_or_else(|| {
        fail("This sign-in was already finished or belongs to an earlier session. Start again.")
    })?;
    // Supabase accepts a code for five minutes.
    if flow.started.elapsed() > Duration::from_secs(300) {
        return Err(fail("Sign-in took too long and expired. Start again."));
    }
    token(
        cloud,
        "pkce",
        json!({"auth_code":code,"code_verifier":flow.verifier.as_str()}),
    )
}
/// Revokes one session on this device only. Best effort.
pub(crate) fn revoke(cloud: &Transport, access: &str) {
    let _ = cloud.send(
        cloud
            .request(Method::POST, "/auth/v1/logout?scope=local")
            .bearer_auth(access),
    );
}
/// Opening the email link signs the browser in instead; the address it lands on keeps that session.
fn redirect_session(link: &str) -> Result<Option<Zeroizing<String>>> {
    let Some(fragment) = reqwest::Url::parse(link)
        .ok()
        .and_then(|u| u.fragment().map(str::to_owned))
    else {
        return Ok(None);
    };
    let mut pairs =
        reqwest::Url::parse("http://fragment.invalid/").map_err(|_| fail("Invalid link."))?;
    pairs.set_query(Some(&fragment));
    let mut refresh = None;
    for (key, value) in pairs.query_pairs() {
        if key == "error_description" {
            return Err(fail(&format!(
                "Supabase rejected that sign-in link: {value}. Request a new sign-in email."
            )));
        }
        if key == "refresh_token" {
            refresh = Some(Zeroizing::new(value.into_owned()));
        }
    }
    Ok(refresh)
}
fn columns(c: &Connection, t: &str) -> Result<Vec<(String, bool)>> {
    let mut s = c.prepare(&format!("PRAGMA table_info(\"{t}\")"))?;
    let rows = s.query_map([], |r| Ok((r.get(1)?, r.get::<_, i64>(5)? > 0)))?;
    Ok(rows.collect::<std::result::Result<_, _>>()?)
}
fn table(t: &str) -> Result<()> {
    if TABLES.contains(&t) {
        Ok(())
    } else {
        Err(fail("Unsupported cloud record type."))
    }
}
fn key_expr(cols: &[(String, bool)], prefix: &str) -> String {
    let mut cols = cols.to_vec();
    cols.sort_by(|a, b| a.0.cmp(&b.0));
    format!(
        "json_object({})",
        cols.iter()
            .filter(|c| c.1)
            .map(|c| format!("'{}',{prefix}.\"{}\"", c.0, c.0))
            .collect::<Vec<_>>()
            .join(",")
    )
}
fn row_expr(cols: &[(String, bool)], prefix: &str) -> String {
    format!(
        "json_object({})",
        cols.iter()
            .map(|c| format!("'{}',{prefix}.\"{}\"", c.0, c.0))
            .collect::<Vec<_>>()
            .join(",")
    )
}
fn canonical_key(c: &Connection, t: &str, k: &Value) -> Result<String> {
    let mut m = Map::new();
    for (name, pk) in columns(c, t)? {
        if pk {
            m.insert(
                name.clone(),
                k.get(&name)
                    .cloned()
                    .ok_or_else(|| fail("Cloud key is incomplete."))?,
            );
        }
    }
    Ok(serde_json::to_string(&m)?)
}
fn sql_value(v: &Value) -> Result<SqlValue> {
    Ok(match v {
        Value::Null => SqlValue::Null,
        Value::String(s) => SqlValue::Text(s.clone()),
        Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                SqlValue::Integer(i)
            } else {
                SqlValue::Real(n.as_f64().ok_or_else(|| fail("Invalid cloud number."))?)
            }
        }
        _ => return Err(fail("Cloud row contains an unsupported column value.")),
    })
}
fn row(c: &Connection, t: &str, key: &Value) -> Result<Option<Value>> {
    table(t)?;
    let cols = columns(c, t)?;
    let keys: Vec<_> = cols.iter().filter(|x| x.1).collect();
    let where_sql = keys
        .iter()
        .map(|x| format!("\"{}\"=?", x.0))
        .collect::<Vec<_>>()
        .join(" AND ");
    let values = keys
        .iter()
        .map(|x| sql_value(&key[&x.0]))
        .collect::<Result<Vec<_>>>()?;
    let sql = format!(
        "SELECT {} FROM \"{t}\" r WHERE {where_sql}",
        row_expr(&cols, "r")
    );
    let data: Option<String> = c
        .query_row(&sql, rusqlite::params_from_iter(values), |r| r.get(0))
        .optional()?;
    data.map(|s| Ok(serde_json::from_str(&s)?)).transpose()
}
fn apply(c: &Connection, record: &Value) -> Result<()> {
    let t = record["entity"]
        .as_str()
        .ok_or_else(|| fail("Missing cloud entity."))?;
    table(t)?;
    let cols = columns(c, t)?;
    let key = &record["key"];
    let keys: Vec<_> = cols.iter().filter(|x| x.1).collect();
    if record["deleted"].as_bool() == Some(true) {
        let predicates = keys
            .iter()
            .map(|x| format!("\"{}\"=?", x.0))
            .collect::<Vec<_>>()
            .join(" AND ");
        let values = keys
            .iter()
            .map(|x| sql_value(&key[&x.0]))
            .collect::<Result<Vec<_>>>()?;
        c.execute(
            &format!("DELETE FROM \"{t}\" WHERE {predicates}"),
            rusqlite::params_from_iter(values),
        )?;
    } else {
        let data = record["data"]
            .as_object()
            .ok_or_else(|| fail("Missing cloud payload."))?;
        let names = cols
            .iter()
            .map(|x| format!("\"{}\"", x.0))
            .collect::<Vec<_>>()
            .join(",");
        let values = cols
            .iter()
            .map(|x| {
                sql_value(
                    data.get(&x.0)
                        .ok_or_else(|| fail("Cloud payload is missing a column."))?,
                )
            })
            .collect::<Result<Vec<_>>>()?;
        let updates = cols
            .iter()
            .filter(|x| !x.1)
            .map(|x| format!("\"{0}\"=excluded.\"{0}\"", x.0))
            .collect::<Vec<_>>()
            .join(",");
        let identity = keys
            .iter()
            .map(|x| format!("\"{}\"", x.0))
            .collect::<Vec<_>>()
            .join(",");
        let action = if updates.is_empty() {
            "DO NOTHING".into()
        } else {
            format!("DO UPDATE SET {updates}")
        };
        // Naming the identity keeps a clash on another unique column (a receipt's
        // checksum, a claim number) from silently rewriting a different local record.
        c.execute(
            &format!(
                "INSERT INTO \"{t}\"({names}) VALUES({}) ON CONFLICT({identity}) {}",
                vec!["?"; cols.len()].join(","),
                action
            ),
            rusqlite::params_from_iter(values),
        )
        .map_err(|e| match e.sqlite_error_code() {
            Some(rusqlite::ErrorCode::ConstraintViolation) => fail(&format!(
                "A cloud {} record clashes with a different record on this device. Nothing was overwritten; review the duplicate before syncing again.",
                t.replace('_', " ").trim_end_matches('s')
            )),
            _ => e.into(),
        })?;
    }
    Ok(())
}
fn set_version(c: &Connection, t: &str, k: &str, r: i64) -> Result<()> {
    c.execute("INSERT INTO cloud_versions VALUES(?1,?2,?3) ON CONFLICT(entity,record_key) DO UPDATE SET revision=excluded.revision",params![t,k,r])?;
    Ok(())
}
/// How long to leave the cloud alone after a failed cycle.
fn retry_delay(failure: Option<Failure>, attempts: i64, retry_after: Option<u64>) -> i64 {
    match failure {
        Some(Failure::Retry) => {
            let base = 5_i64 << attempts.clamp(0, 8);
            // Jitter keeps a fleet of devices from retrying in step.
            let jitter = 80 + i64::from(uuid::Uuid::new_v4().as_bytes()[0]) * 40 / 255;
            (base.min(900) * jitter / 100).max(1)
        }
        Some(Failure::Quota) => retry_after.unwrap_or(60).clamp(30, 3600) as i64,
        // Repeating a refused or rejected request cannot help; wait for the user.
        Some(Failure::Auth | Failure::Permission | Failure::Validation) => 900,
        Some(Failure::Conflict) | None => 60,
    }
}
impl AppService {
    // Fresh databases have independent slots. A full database copy retains this identity.
    fn vault_id(&self) -> Result<String> {
        Ok(self
            .conn()?
            .query_row("SELECT vault_id FROM cloud_control WHERE id=1", [], |r| {
                r.get(0)
            })?)
    }
    fn vault(&self) -> Result<Box<dyn crate::security::secrets::SecretStore>> {
        self.cloud.vault(&self.vault_id()?)
    }
    fn refresh_secret(&self) -> Result<Option<Zeroizing<String>>> {
        self.vault()?
            .get()
            .map_err(|_| fail("Cannot unlock the secure cloud session."))
    }
    /// The one cloud account this workspace belongs to, once it has been linked.
    pub(crate) fn cloud_owner(&self) -> Result<Option<String>> {
        Ok(self
            .conn()?
            .query_row("SELECT owner_id FROM cloud_control WHERE id=1", [], |r| {
                r.get(0)
            })?)
    }
    /// Whether this device already holds records that would join an account.
    pub(crate) fn has_user_data(&self) -> Result<bool> {
        let c = self.conn()?;
        for t in USER_DATA {
            if c.query_row(&format!("SELECT EXISTS(SELECT 1 FROM \"{t}\")"), [], |r| {
                r.get(0)
            })? {
                return Ok(true);
            }
        }
        Ok(false)
    }
    /// Gives this workspace to one cloud account for good. Records already here
    /// are snapshotted first, stay on this device, and upload on the next sync.
    /// Safe to repeat: an interrupted adoption simply runs again.
    pub(crate) fn bind_owner(&self, user_id: &str) -> Result<()> {
        uuid::Uuid::parse_str(user_id).map_err(|_| fail("Invalid cloud account."))?;
        match self.cloud_owner()? {
            Some(owner) if owner == user_id => return Ok(()),
            Some(_) => return Err(fail("This workspace belongs to a different cloud account.")),
            None => {}
        }
        if self.has_user_data()? {
            let backup = self.paths.resolve(&format!(
                "database/before-account-{}.sqlite",
                chrono::Utc::now().format("%Y%m%dT%H%M%S%3fZ")
            ))?;
            // A consistent copy that includes the WAL: https://www.sqlite.org/lang_vacuum.html#vacuuminto
            self.conn()?
                .execute("VACUUM INTO ?1", [backup.to_string_lossy()])?;
        }
        self.conn()?.execute(
            "UPDATE cloud_control SET owner_id=?1 WHERE id=1 AND owner_id IS NULL",
            [user_id],
        )?;
        Ok(())
    }
    /// Stores a session for the account that owns this workspace. Any other identity is refused.
    pub(crate) fn accept_session(&self, session: &Session) -> Result<()> {
        let mut guard = self
            .cloud_lock
            .lock()
            .map_err(|_| fail("Cloud session is busy."))?;
        if self.cloud_owner()?.as_deref() != Some(session.user_id.as_str()) {
            return Err(fail("This workspace belongs to a different cloud account."));
        }
        self.vault()?
            .set(&session.refresh)
            .map_err(|_| fail("Unable to save the cloud session in the native vault."))?;
        *guard = Some(session.cached());
        self.conn()?.execute(
            "UPDATE cloud_control SET email=coalesce(?1,email),providers=?2,last_error=NULL,failure=NULL,attempts=0,retry_at=NULL,request_id=NULL WHERE id=1",
            params![session.email, session.providers.join(",")],
        )?;
        Ok(())
    }
    /// Stops a running sync at its next step and discards its late responses.
    pub(crate) fn cloud_cancel(&self) {
        self.cloud_generation.fetch_add(1, Ordering::SeqCst);
    }
    fn live(&self, generation: u64) -> Result<()> {
        if self.cloud_generation.load(Ordering::SeqCst) == generation {
            Ok(())
        } else {
            Err(cancelled())
        }
    }
    pub(crate) fn initialize_cloud(&self) -> Result<()> {
        let c = self.conn()?;
        for t in TABLES {
            let cols = columns(&c, t)?;
            for (kind, prefix, op) in [
                ("INSERT", "NEW", "upsert"),
                ("UPDATE", "NEW", "upsert"),
                ("DELETE", "OLD", "delete"),
            ] {
                let key = key_expr(&cols, prefix);
                let data = if op == "delete" {
                    "'{}'".into()
                } else {
                    row_expr(&cols, prefix)
                };
                c.execute_batch(&format!("DROP TRIGGER IF EXISTS cloud_{t}_{kind}; CREATE TRIGGER cloud_{t}_{kind} AFTER {kind} ON \"{t}\" WHEN (SELECT applying FROM cloud_control WHERE id=1)=0 BEGIN INSERT INTO cloud_pending(entity,record_key,change_id,base_revision,operation,data) VALUES('{t}',{key},(lower(hex(randomblob(4)))||'-'||lower(hex(randomblob(2)))||'-'||lower(hex(randomblob(2)))||'-'||lower(hex(randomblob(2)))||'-'||lower(hex(randomblob(6)))),coalesce((SELECT revision FROM cloud_versions WHERE entity='{t}' AND record_key={key}),0),'{op}',{data}) ON CONFLICT(entity,record_key) DO UPDATE SET change_id=excluded.change_id,base_revision=excluded.base_revision,operation=excluded.operation,data=excluded.data; END;"))?;
            }
        }
        Ok(())
    }
    pub fn cloud_status(&self) -> Result<CloudStatus> {
        let signed_in = self.refresh_secret()?.is_some();
        let c = self.conn()?;
        type Row = (
            Option<String>,
            Option<String>,
            Option<String>,
            Option<String>,
            Option<String>,
            Option<String>,
            Option<String>,
            Option<String>,
        );
        let (owner, email, providers, last_sync, error, failure, retry_at, request_id): Row = c.query_row(
            "SELECT owner_id,email,providers,last_sync,last_error,failure,retry_at,request_id FROM cloud_control WHERE id=1",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?, r.get(6)?, r.get(7)?)),
        )?;
        let pending: i64 = c.query_row("SELECT count(*) FROM cloud_pending", [], |r| r.get(0))?;
        let conflicts: i64 =
            c.query_row("SELECT count(*) FROM cloud_conflicts", [], |r| r.get(0))?;
        let class = failure.as_deref().and_then(Failure::from_code);
        let state = match (owner.is_some(), signed_in, class) {
            (false, _, _) => "local",
            (true, false, Some(Failure::Auth)) => "signInAgain",
            (true, false, _) => "signedOut",
            (true, true, Some(Failure::Auth)) => "signInAgain",
            (true, true, Some(Failure::Retry | Failure::Quota)) => "offline",
            (true, true, _) if error.is_some() || conflicts > 0 => "needsAttention",
            (true, true, _) if pending > 0 => "pending",
            (true, true, _) => "upToDate",
        };
        Ok(CloudStatus {
            signed_in,
            linked: owner.is_some(),
            email,
            providers: providers
                .unwrap_or_default()
                .split(',')
                .filter(|p| !p.is_empty())
                .map(str::to_owned)
                .collect(),
            pending,
            conflicts,
            last_sync,
            error,
            state,
            failure,
            retry_at,
            request_id,
        })
    }
    pub fn cloud_sign_out(&self) -> Result<CloudStatus> {
        self.cloud_cancel();
        let mut guard = self
            .cloud_lock
            .lock()
            .map_err(|_| fail("Cloud session is busy."))?;
        if let Some(owner) = self.cloud_owner()? {
            if let Ok(token) = self.cloud_access(&mut guard, &owner) {
                revoke(&self.cloud, &token);
            }
        }
        *guard = None;
        self.vault()?
            .delete()
            .map_err(|_| fail("Unable to clear the secure cloud session."))?;
        self.conn()?.execute(
            "UPDATE cloud_control SET last_error=NULL,failure=NULL,attempts=0,retry_at=NULL,request_id=NULL WHERE id=1",
            [],
        )?;
        self.cloud_status()
    }
    /// A current access token for `owner`. Called only under `cloud_lock`, so one
    /// refresh runs at a time and its rotated credential is saved before any other use.
    fn cloud_access(&self, cache: &mut Option<Access>, owner: &str) -> Result<Zeroizing<String>> {
        if let Some(a) = cache.as_ref().filter(|a| Instant::now() < a.until) {
            return Ok(a.token.clone());
        }
        let refresh = self.refresh_secret()?.ok_or_else(|| {
            AppError::new(
                Failure::Auth.code(),
                "Sign in to enable cloud synchronization.",
            )
        })?;
        // Supabase revokes the session if a spent refresh token returns after 10 s, so a
        // lost reply is retried at once with a short timeout instead of at the next sync.
        let request = || {
            self.cloud.json(
                self.cloud
                    .request(Method::POST, "/auth/v1/token?grant_type=refresh_token")
                    .timeout(Duration::from_secs(6))
                    .json(&json!({"refresh_token":refresh.as_str()})),
            )
        };
        let mut reply = request();
        if reply
            .as_ref()
            .is_err_and(|e| e.code == Failure::Retry.code())
        {
            reply = request();
        }
        let value = match reply {
            Ok(value) => value,
            Err(e) => {
                let ended = [
                    "refresh_token_not_found",
                    "refresh_token_already_used",
                    "session_not_found",
                    "session_expired",
                    "invalid_grant",
                    "user_not_found",
                    "user_banned",
                ];
                let reason = self.cloud.last_failure().error_code.unwrap_or_default();
                if e.code != Failure::Retry.code()
                    && e.code != Failure::Quota.code()
                    && ended.contains(&reason.as_str())
                {
                    // Revoked or already used: it can never work again.
                    *cache = None;
                    let _ = self.vault()?.delete();
                    return Err(AppError::new(
                        Failure::Auth.code(),
                        "Your cloud session ended. Sign in again to resume sync; local changes are kept.",
                    ));
                }
                return Err(e);
            }
        };
        let session = Session::parse(value)?;
        if session.user_id != owner {
            return Err(AppError::new(
                Failure::Auth.code(),
                "Cloud owner verification failed.",
            ));
        }
        self.vault()?
            .set(&session.refresh)
            .map_err(|_| fail("Unable to refresh the secure session."))?;
        *cache = Some(session.cached());
        Ok(session.access.clone())
    }
    /// A signed-in access token for adding a sign-in method to this account.
    pub(crate) fn cloud_token(&self) -> Result<Zeroizing<String>> {
        let mut guard = self
            .cloud_lock
            .lock()
            .map_err(|_| fail("Cloud session is busy."))?;
        let owner = self
            .cloud_owner()?
            .ok_or_else(|| fail("Sign in before adding another sign-in method."))?;
        self.cloud_access(&mut guard, &owner)
    }
    /// One sync cycle. Automatic callers pass `force = false` and are held off
    /// while a failure is backing off; a person pressing Retry passes `true`.
    pub fn cloud_sync(&self, force: bool) -> Result<CloudStatus> {
        let mut guard = match self.cloud_lock.try_lock() {
            Ok(g) => g,
            Err(_) => return self.cloud_status(),
        };
        let generation = self.cloud_generation.load(Ordering::SeqCst);
        let waiting: bool = self.conn()?.query_row(
            "SELECT coalesce(retry_at>?1,0) FROM cloud_control WHERE id=1",
            [chrono::Utc::now().to_rfc3339()],
            |r| r.get(0),
        )?;
        if waiting && !force {
            return self.cloud_status();
        }
        let mut result = self.sync_cycle(&mut guard, generation);
        if result
            .as_ref()
            .is_err_and(|e| e.code == Failure::Auth.code())
            && self.refresh_secret()?.is_some()
        {
            // The server refused the access token: renew it once and try again.
            *guard = None;
            result = self.sync_cycle(&mut guard, generation);
        }
        if self.live(generation).is_err() {
            *guard = None;
            return self.cloud_status();
        }
        match result {
            Ok(()) => {
                self.conn()?.execute(
                    "UPDATE cloud_control SET last_sync=?1,last_error=NULL,failure=NULL,attempts=0,retry_at=NULL,request_id=NULL WHERE id=1",
                    [chrono::Utc::now().to_rfc3339()],
                )?;
            }
            Err(e) => {
                let failure = Failure::from_code(&e.code);
                // Request details describe transport failures only.
                let detail = failure
                    .map(|_| self.cloud.last_failure())
                    .unwrap_or_default();
                let c = self.conn()?;
                let attempts: i64 =
                    c.query_row("SELECT attempts FROM cloud_control WHERE id=1", [], |r| {
                        r.get(0)
                    })?;
                let retry_at = chrono::Utc::now()
                    + chrono::Duration::seconds(retry_delay(failure, attempts, detail.retry_after));
                c.execute(
                    "UPDATE cloud_control SET last_error=?1,failure=?2,attempts=attempts+1,retry_at=?3,request_id=?4 WHERE id=1",
                    params![e.message, e.code, retry_at.to_rfc3339(), detail.request_id],
                )?;
            }
        }
        self.cloud_status()
    }
    /// What Settings shows when sync misbehaves: each layer checked separately,
    /// with nothing that identifies a record or carries a credential.
    pub fn cloud_diagnose(&self) -> Result<Value> {
        let config = &self.cloud.config;
        // Reaching the project proves nothing about being allowed to sync.
        let reachable = self
            .cloud
            .send(self.cloud.request(Method::GET, "/auth/v1/health"))
            .map(|_| "ok".to_string())
            .unwrap_or_else(|e| e.message);
        let authenticated = match (self.cloud_owner()?, self.cloud_lock.try_lock()) {
            (None, _) => "not signed in".to_string(),
            (_, Err(_)) => "sync in progress".to_string(),
            (Some(owner), Ok(mut guard)) => {
                let cursor: i64 = self.conn()?.query_row(
                    "SELECT cursor FROM cloud_control WHERE id=1",
                    [],
                    |r| r.get(0),
                )?;
                // The same call sync makes, so this checks the session, grants and protocol together.
                self.cloud_access(&mut guard, &owner)
                    .and_then(|token| {
                        self.cloud.rpc(
                            &token,
                            "folio_sync_pull",
                            json!({"since":cursor,"page_limit":1}),
                        )
                    })
                    .and_then(|page| {
                        if page["records"].is_array() && page["cursor"].is_i64() {
                            Ok("ok".to_string())
                        } else {
                            Err(fail("This Folio version does not understand the cloud's sync format. Update Folio."))
                        }
                    })
                    .unwrap_or_else(|e| e.message)
            }
        };
        let files: i64 = self.conn()?.query_row(
            "SELECT count(*) FROM receipt_files f WHERE NOT EXISTS(SELECT 1 FROM cloud_uploaded u WHERE u.relative_path=f.relative_path AND u.sha256=f.sha256)",
            [],
            |r| r.get(0),
        )?;
        Ok(json!({
            "environment": config.environment,
            "project": config.host(),
            "keyKind": config.key_kind(),
            "callback": config.callback,
            "reachable": reachable,
            "authenticated": authenticated,
            "pendingFiles": files,
            "status": self.cloud_status()?,
        }))
    }
    fn sync_cycle(&self, cache: &mut Option<Access>, generation: u64) -> Result<()> {
        let owner = self
            .cloud_owner()?
            .ok_or_else(|| fail("Sign in to enable cloud synchronization."))?;
        let token = self.cloud_access(cache, &owner)?;
        // A receipt both devices imported is reconciled by the pull that follows
        // its rejection, so one more round finishes the upload.
        for round in 0..2 {
            self.pull(&token, generation)?;
            self.seed()?;
            let pending = self.pending()?;
            if pending.is_empty() {
                break;
            }
            // Remember what is in flight: if the reply is lost, the next pull
            // recognises these values as this device's own change.
            self.conn()?.execute(
                "UPDATE cloud_pending SET sent=json_object('id',change_id,'operation',operation,'data',json(data)) WHERE change_id IN (SELECT value FROM json_each(?1))",
                [serde_json::to_string(
                    &pending.iter().map(|p| p["changeId"].clone()).collect::<Vec<_>>(),
                )?],
            )?;
            let replies = self
                .cloud
                .rpc(&token, "folio_sync_push", json!({"changes":pending}))?;
            self.live(generation)?;
            let mut duplicate = false;
            let mut c = self.conn()?;
            let tx = c.transaction()?;
            for reply in replies
                .as_array()
                .ok_or_else(|| fail("Invalid push response."))?
            {
                let id = reply["changeId"]
                    .as_str()
                    .ok_or_else(|| fail("Missing push acknowledgement."))?;
                let sent = pending
                    .iter()
                    .find(|p| p["changeId"] == id)
                    .ok_or_else(|| fail("Unknown push acknowledgement."))?;
                let t = sent["entity"].as_str().unwrap();
                let key = canonical_key(&tx, t, &sent["key"])?;
                match reply["status"].as_str() {
                    Some("applied") => {
                        let revision = reply["revision"]
                            .as_i64()
                            .ok_or_else(|| fail("Missing revision."))?;
                        set_version(&tx, t, &key, revision)?;
                        // An edit made while this request was in flight has a new change ID and stays.
                        tx.execute("DELETE FROM cloud_pending WHERE change_id=?1", [id])?;
                        tx.execute("UPDATE cloud_pending SET base_revision=?1,sent=NULL WHERE entity=?2 AND record_key=?3",params![revision,t,key])?;
                    }
                    Some("conflict") => {
                        record_conflict(&tx, t, &key, &sent["data"], &reply["record"])?;
                    }
                    Some("blocked") => {}
                    Some("rejected") if reply["reason"] == "duplicate_receipt" => duplicate = true,
                    _ => return Err(fail("Unknown cloud push result.")),
                }
            }
            tx.commit()?;
            if !duplicate {
                break;
            }
            if round == 1 {
                return Err(fail("The same receipt was imported on another device and could not be matched automatically. Your local edits are preserved; compare both expense records before retrying sync."));
            }
        }
        self.pull(&token, generation)?;
        self.transfer_assets(&token, &owner, generation)?;
        Ok(())
    }
    fn pending(&self) -> Result<Vec<Value>> {
        let c = self.conn()?;
        // An expense still being read here would look permanently stuck on another
        // device, which has no job for it. Upload once extraction settles.
        let extracting: bool = c.query_row(
            "SELECT EXISTS(SELECT 1 FROM cloud_pending p JOIN jobs j ON j.entity_id=json_extract(p.record_key,'$.id') WHERE p.entity='expenses' AND json_extract(p.data,'$.status')='extracting' AND j.job_type='extract_receipt' AND j.status IN ('pending','running'))",
            [],
            |r| r.get(0),
        )?;
        if extracting {
            return Ok(Vec::new());
        }
        let mut s=c.prepare("SELECT entity,record_key,change_id,base_revision,operation,data FROM cloud_pending WHERE NOT EXISTS(SELECT 1 FROM cloud_conflicts f WHERE f.entity=cloud_pending.entity AND f.record_key=cloud_pending.record_key) ORDER BY rowid LIMIT 2001")?;
        let rows = s.query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, i64>(3)?,
                r.get::<_, String>(4)?,
                r.get::<_, String>(5)?,
            ))
        })?;
        let mut result = Vec::new();
        for r in rows {
            let (t, k, id, v, op, d) = r?;
            result.push(json!({"entity":t,"key":serde_json::from_str::<Value>(&k)?,"changeId":id,"baseRevision":v,"operation":op,"data":serde_json::from_str::<Value>(&d)?}));
        }
        if result.len() > 2000 || serde_json::to_vec(&result)?.len() > 4 * 1024 * 1024 {
            return Err(fail("The pending change set exceeds one safe cloud transaction (2,000 records or 4 MB). Export your data and contact support to split it safely; no changes were discarded."));
        }
        Ok(result)
    }
    fn seed(&self) -> Result<()> {
        let mut c = self.conn()?;
        let tx = c.transaction()?;
        let done: i64 = tx.query_row(
            "SELECT bootstrapped FROM cloud_control WHERE id=1",
            [],
            |r| r.get(0),
        )?;
        if done == 0 {
            for t in TABLES {
                let cols = columns(&tx, t)?;
                let key = key_expr(&cols, "r");
                let data = row_expr(&cols, "r");
                tx.execute_batch(&format!("INSERT OR IGNORE INTO cloud_pending(entity,record_key,change_id,base_revision,operation,data) SELECT '{t}',{key},(lower(hex(randomblob(4)))||'-'||lower(hex(randomblob(2)))||'-'||lower(hex(randomblob(2)))||'-'||lower(hex(randomblob(2)))||'-'||lower(hex(randomblob(6)))),0,'upsert',{data} FROM \"{t}\" r WHERE NOT EXISTS(SELECT 1 FROM cloud_versions v WHERE v.entity='{t}' AND v.record_key={key});"))?;
            }
            tx.execute("UPDATE cloud_control SET bootstrapped=1 WHERE id=1", [])?;
        }
        tx.commit()?;
        Ok(())
    }
    fn pull(&self, token: &str, generation: u64) -> Result<()> {
        for _ in 0..100 {
            let since: i64 =
                self.conn()?
                    .query_row("SELECT cursor FROM cloud_control WHERE id=1", [], |r| {
                        r.get(0)
                    })?;
            let page = self.cloud.rpc(
                token,
                "folio_sync_pull",
                json!({"since":since,"page_limit":500}),
            )?;
            // A reply that arrives after sign-out or an account change is dropped.
            self.live(generation)?;
            let all_records = page["records"]
                .as_array()
                .ok_or_else(|| fail("Invalid pull response."))?;
            let mut latest = std::collections::BTreeMap::new();
            for record in all_records {
                latest.insert(
                    format!("{}:{}", record["entity"], record["key"]),
                    record.clone(),
                );
            }
            let records: Vec<Value> = latest.into_values().collect();
            let cursor = page["cursor"]
                .as_i64()
                .ok_or_else(|| fail("Missing cloud cursor."))?;
            if cursor < since {
                return Err(fail("Cloud cursor moved backwards."));
            }
            let mut c = self.conn()?;
            let tx = c.transaction()?;
            tx.execute_batch(
                "PRAGMA defer_foreign_keys=ON; UPDATE cloud_control SET applying=1 WHERE id=1;",
            )?;
            self.prune_default_workspace(&tx, &records)?;
            // Receipts first: an expense is matched through its receipt.
            for entity in ["receipt_files", "expenses"] {
                for remote in records.iter().filter(|r| r["entity"] == entity) {
                    self.adopt_duplicate(&tx, remote)?;
                }
            }
            let mut conflict = false;
            for remote in &records {
                if let Some(local) = self.remote_conflict(&tx, remote)? {
                    let t = remote["entity"].as_str().unwrap();
                    let key = canonical_key(&tx, t, &remote["key"])?;
                    record_conflict(&tx, t, &key, &local, remote)?;
                    conflict = true;
                }
            }
            if conflict {
                tx.execute("UPDATE cloud_control SET applying=0 WHERE id=1", [])?;
                tx.commit()?;
                return Err(fail("Cloud changes need conflict review. The complete remote transaction is held until you choose which version to keep."));
            }
            for remote in &records {
                self.merge_remote(&tx, remote)?;
            }
            tx.execute(
                "UPDATE cloud_control SET applying=0,cursor=?1 WHERE id=1",
                [cursor],
            )?;
            tx.commit()?;
            if page["hasMore"].as_bool() != Some(true) {
                return Ok(());
            }
            if cursor == since {
                return Err(fail("Cloud pagination made no progress."));
            }
        }
        Err(fail("Cloud catch-up continues on the next sync."))
    }
    fn remote_conflict(&self, c: &Connection, remote: &Value) -> Result<Option<Value>> {
        let t = remote["entity"]
            .as_str()
            .ok_or_else(|| fail("Missing entity."))?;
        table(t)?;
        let key = canonical_key(c, t, &remote["key"])?;
        let known: Option<i64> = c
            .query_row(
                "SELECT revision FROM cloud_versions WHERE entity=?1 AND record_key=?2",
                params![t, key],
                |r| r.get(0),
            )
            .optional()?;
        let rev = remote["revision"]
            .as_i64()
            .ok_or_else(|| fail("Missing revision."))?;
        if known.is_some_and(|v| v >= rev) || own_echo(c, t, &key, remote)? {
            return Ok(None);
        }
        let selected:Option<(Option<String>,String,String)>=c.query_row("SELECT choice,remote_record,local_data FROM cloud_conflicts WHERE entity=?1 AND record_key=?2",params![t,key],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).optional()?;
        let current_local = row(c, t, &remote["key"])?;
        let forced = selected.is_some_and(|(choice, saved, selected_local)| {
            choice.as_deref() == Some("remote")
                && serde_json::from_str::<Value>(&selected_local)
                    .is_ok_and(|v| v == current_local.clone().unwrap_or(Value::Null))
                && serde_json::from_str::<Value>(&saved)
                    .is_ok_and(|r| r["revision"] == remote["revision"])
        });
        if forced {
            return Ok(None);
        }
        let local = row(c, t, &remote["key"])?;
        let same = same_row(local.as_ref(), remote);
        let pending: bool = c.query_row(
            "SELECT EXISTS(SELECT 1 FROM cloud_pending WHERE entity=?1 AND record_key=?2)",
            params![t, key],
            |r| r.get(0),
        )?;
        if !same && (pending || (known.is_none() && local.is_some())) {
            Ok(Some(local.unwrap_or(Value::Null)))
        } else {
            Ok(None)
        }
    }
    /// The same receipt imported on two devices before either synced has two
    /// identities. The copy the cloud has never seen takes the cloud's identity:
    /// its file and its expense's own values are kept, and any difference in the
    /// expense is then reviewed as an ordinary conflict instead of being lost.
    fn adopt_duplicate(&self, c: &Connection, remote: &Value) -> Result<()> {
        if remote["deleted"] == true {
            return Ok(());
        }
        let (Some(t), Some(cloud_id)) = (remote["entity"].as_str(), remote["key"]["id"].as_str())
        else {
            return Ok(());
        };
        let (column, value) = match t {
            "receipt_files" => ("sha256", &remote["data"]["sha256"]),
            _ => ("receipt_id", &remote["data"]["receipt_id"]),
        };
        let Some(value) = value.as_str() else {
            return Ok(());
        };
        let local: Option<String> = c
            .query_row(
                &format!("SELECT id FROM \"{t}\" WHERE \"{column}\"=?1 AND id<>?2 AND NOT EXISTS(SELECT 1 FROM \"{t}\" WHERE id=?2)"),
                params![value, cloud_id],
                |r| r.get(0),
            )
            .optional()?;
        let Some(local_id) = local else {
            return Ok(());
        };
        let local_key = serde_json::to_string(&json!({"id":local_id}))?;
        let known: bool = c.query_row(
            "SELECT EXISTS(SELECT 1 FROM cloud_versions WHERE entity=?1 AND record_key=?2)",
            params![t, local_key],
            |r| r.get(0),
        )?;
        if known {
            // Already on the cloud under its own identity; the clash is reported, not rewritten.
            return Ok(());
        }
        c.execute(
            "DELETE FROM cloud_pending WHERE entity=?1 AND record_key=?2",
            params![t, local_key],
        )?;
        if t == "receipt_files" {
            // Copied, not moved: if this transaction rolls back the original is untouched,
            // and startup storage recovery removes whichever folder ends up unreferenced.
            let (from, to) = (
                self.paths.resolve(&format!("receipts/{local_id}"))?,
                self.paths.resolve(&format!("receipts/{cloud_id}"))?,
            );
            uuid::Uuid::parse_str(cloud_id)
                .map_err(|_| fail("Cloud receipt path does not match its identity."))?;
            if from.is_dir() {
                fs::create_dir_all(&to)?;
                for entry in fs::read_dir(&from)? {
                    let entry = entry?;
                    let target = to.join(entry.file_name());
                    if entry.file_type()?.is_file() && !target.exists() {
                        fs::copy(entry.path(), target)?;
                    }
                }
            }
            c.execute(
                "UPDATE receipt_files SET id=?2,relative_path='receipts/'||?2||substr(relative_path,length('receipts/'||?1)+1) WHERE id=?1",
                params![local_id, cloud_id],
            )?;
            c.execute(
                "DELETE FROM cloud_uploaded WHERE relative_path LIKE 'receipts/'||?1||'/%'",
                [&local_id],
            )?;
            // The bytes are identical, so the cloud's file name and dates are taken as they are.
            apply(c, remote)?;
            // The expense keeps its own values; following its receipt is a local edit to upload.
            c.execute_batch("UPDATE cloud_control SET applying=0 WHERE id=1")?;
            c.execute(
                "UPDATE expenses SET receipt_id=?2 WHERE receipt_id=?1",
                params![local_id, cloud_id],
            )?;
        } else {
            c.execute(
                "DELETE FROM cloud_pending WHERE entity IN ('claim_expenses','receipt_item_reviews') AND json_extract(record_key,'$.expense_id')=?1",
                [&local_id],
            )?;
            c.execute(
                "UPDATE expenses SET id=?2 WHERE id=?1",
                params![local_id, cloud_id],
            )?;
            c.execute(
                "UPDATE jobs SET entity_id=?2 WHERE entity_id=?1",
                params![local_id, cloud_id],
            )?;
            // Everything that pointed at the local expense follows it and uploads, and
            // touching the expense queues its local values against the cloud's version.
            c.execute_batch("UPDATE cloud_control SET applying=0 WHERE id=1")?;
            for table in [
                "extraction_runs",
                "claim_expenses",
                "receipt_item_reviews",
                "transactions",
            ] {
                c.execute(
                    &format!("UPDATE \"{table}\" SET expense_id=?2 WHERE expense_id=?1"),
                    params![local_id, cloud_id],
                )?;
            }
            c.execute(
                "UPDATE audit_events SET entity_id=?2 WHERE entity_id=?1",
                params![local_id, cloud_id],
            )?;
            c.execute("UPDATE expenses SET id=id WHERE id=?1", [cloud_id])?;
        }
        c.execute_batch("UPDATE cloud_control SET applying=1 WHERE id=1")?;
        Ok(())
    }
    fn prune_default_workspace(&self, c: &Connection, records: &[Value]) -> Result<()> {
        let initial: i64 = c.query_row(
            "SELECT bootstrapped FROM cloud_control WHERE id=1",
            [],
            |r| r.get(0),
        )?;
        if initial != 0 {
            return Ok(());
        }
        let remote_personal = records.iter().find(|r| {
            r["entity"] == "workspaces" && r["data"]["kind"] == "personal" && r["deleted"] != true
        });
        let Some(remote) = remote_personal else {
            return Ok(());
        };
        let id:Option<String>=c.query_row("SELECT id FROM workspaces WHERE kind='personal' AND name='Personal' AND version=1 AND created_at=updated_at AND (SELECT count(*) FROM workspaces)=1",[],|r|r.get(0)).optional()?;
        let Some(id) = id else {
            return Ok(());
        };
        if remote["key"]["id"] == id {
            return Ok(());
        }
        // Legacy expense/claim records do not reference finance workspaces or categories.
        // Only finance data prevents replacing this untouched, generated workspace.
        for table in [
            "accounts",
            "transactions",
            "bills",
            "budgets",
            "customers",
            "inventory_items",
            "invoices",
            "business_profiles",
            "stock_movements",
            "invoice_payments",
            "transaction_suggestions",
        ] {
            let count: i64 =
                c.query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r.get(0))?;
            if count != 0 {
                return Ok(());
            }
        }
        let total: i64 = c.query_row(
            "SELECT count(*) FROM categories WHERE workspace_id=?1",
            [&id],
            |r| r.get(0),
        )?;
        let untouched:i64=c.query_row("SELECT count(*) FROM categories WHERE workspace_id=?1 AND version=1 AND archived_at IS NULL AND created_at=updated_at AND ((kind='income' AND name IN ('Salary','Business','Reimbursement','Other Income')) OR (kind='expense' AND name IN ('Meals','Transport','Accommodation','Fuel','Parking','Office Supplies','Travel','Entertainment','Software','Bills','Other')))",[&id],|r|r.get(0))?;
        if total != 15 || untouched != 15 {
            return Ok(());
        }
        c.execute("DELETE FROM cloud_pending WHERE entity='categories' AND json_extract(record_key,'$.id') IN (SELECT id FROM categories WHERE workspace_id=?1)",[&id])?;
        c.execute("DELETE FROM cloud_pending WHERE entity='workspaces' AND json_extract(record_key,'$.id')=?1",[&id])?;
        c.execute("DELETE FROM categories WHERE workspace_id=?1", [&id])?;
        c.execute("DELETE FROM workspaces WHERE id=?1", [&id])?;
        Ok(())
    }
    fn merge_remote(&self, c: &Connection, remote: &Value) -> Result<()> {
        let t = remote["entity"]
            .as_str()
            .ok_or_else(|| fail("Missing entity."))?;
        table(t)?;
        let key = canonical_key(c, t, &remote["key"])?;
        let revision = remote["revision"]
            .as_i64()
            .ok_or_else(|| fail("Missing revision."))?;
        let known: Option<i64> = c
            .query_row(
                "SELECT revision FROM cloud_versions WHERE entity=?1 AND record_key=?2",
                params![t, key],
                |r| r.get(0),
            )
            .optional()?;
        if known.is_some_and(|r| r >= revision) {
            return Ok(());
        }
        if own_echo(c, t, &key, remote)? {
            // The server applied a change whose reply never arrived. Adopt its
            // revision; an edit made since then stays pending on top of it.
            set_version(c, t, &key, revision)?;
            c.execute(
                "DELETE FROM cloud_pending WHERE entity=?1 AND record_key=?2 AND change_id=json_extract(sent,'$.id')",
                params![t, key],
            )?;
            c.execute(
                "UPDATE cloud_pending SET base_revision=?3,sent=NULL WHERE entity=?1 AND record_key=?2",
                params![t, key, revision],
            )?;
            return Ok(());
        }
        let selected:Option<(Option<String>,String,String)>=c.query_row("SELECT choice,remote_record,local_data FROM cloud_conflicts WHERE entity=?1 AND record_key=?2",params![t,key],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).optional()?;
        let current_local = row(c, t, &remote["key"])?;
        let forced = selected.is_some_and(|(choice, saved, selected_local)| {
            choice.as_deref() == Some("remote")
                && serde_json::from_str::<Value>(&selected_local)
                    .is_ok_and(|v| v == current_local.clone().unwrap_or(Value::Null))
                && serde_json::from_str::<Value>(&saved)
                    .is_ok_and(|r| r["revision"] == remote["revision"])
        });
        if forced {
            apply(c, remote)?;
            set_version(c, t, &key, revision)?;
            c.execute(
                "DELETE FROM cloud_pending WHERE entity=?1 AND record_key=?2",
                params![t, key],
            )?;
            c.execute(
                "DELETE FROM cloud_conflicts WHERE entity=?1 AND record_key=?2",
                params![t, key],
            )?;
            return Ok(());
        }
        let local = row(c, t, &remote["key"])?;
        let pending: bool = c.query_row(
            "SELECT EXISTS(SELECT 1 FROM cloud_pending WHERE entity=?1 AND record_key=?2)",
            params![t, key],
            |r| r.get(0),
        )?;
        let same = same_row(local.as_ref(), remote);
        if same {
            set_version(c, t, &key, revision)?;
            c.execute(
                "DELETE FROM cloud_pending WHERE entity=?1 AND record_key=?2",
                params![t, key],
            )?;
            return Ok(());
        }
        if pending || (known.is_none() && local.is_some()) {
            record_conflict(c, t, &key, &local.unwrap_or(Value::Null), remote)?;
            return Ok(());
        }
        apply(c, remote)?;
        set_version(c, t, &key, revision)
    }
    pub fn cloud_conflicts(&self) -> Result<Value> {
        let c = self.conn()?;
        let mut s = c.prepare(
            "SELECT id,entity,local_data,remote_record FROM cloud_conflicts ORDER BY rowid",
        )?;
        let rows = s.query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
            ))
        })?;
        let mut list = Vec::new();
        for r in rows {
            let (id, entity, local, remote) = r?;
            list.push(json!({"id":id,"entity":entity,"local":serde_json::from_str::<Value>(&local)?,"remote":serde_json::from_str::<Value>(&remote)?}));
        }
        Ok(Value::Array(list))
    }
    pub fn cloud_resolve_conflict(&self, id: &str, choice: &str) -> Result<CloudStatus> {
        let _guard = self
            .cloud_lock
            .lock()
            .map_err(|_| fail("Cloud session is busy."))?;
        let mut c = self.conn()?;
        let tx = c.transaction()?;
        let (t, key, remote): (String, String, String) = tx.query_row(
            "SELECT entity,record_key,remote_record FROM cloud_conflicts WHERE id=?1",
            [id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )?;
        let record: Value = serde_json::from_str(&remote)?;
        let revision = record["revision"]
            .as_i64()
            .ok_or_else(|| fail("Invalid conflict revision."))?;
        if choice == "remote" {
            // Stage the decision: apply alongside the entire pulled aggregate,
            // which may include newly created parent rows not present locally yet.
            let current = row(&tx, &t, &record["key"])?;
            tx.execute(
                "UPDATE cloud_conflicts SET choice='remote',local_data=?2 WHERE id=?1",
                params![id, serde_json::to_string(&current.unwrap_or(Value::Null))?],
            )?;
            tx.commit()?;
            drop(c);
            return self.cloud_status();
        } else if choice == "local" {
            let current = row(&tx, &t, &record["key"])?;
            tx.execute("INSERT INTO cloud_pending(entity,record_key,change_id,base_revision,operation,data) VALUES(?1,?2,?3,?4,?5,?6) ON CONFLICT(entity,record_key) DO UPDATE SET change_id=excluded.change_id,base_revision=excluded.base_revision,operation=excluded.operation,data=excluded.data",params![t,key,uuid::Uuid::new_v4().to_string(),revision,if current.is_some(){"upsert"}else{"delete"},serde_json::to_string(&current.unwrap_or(json!({})))?])?;
        } else {
            return Err(fail("Choose local or remote to resolve this conflict."));
        }
        set_version(&tx, &t, &key, revision)?;
        tx.execute("DELETE FROM cloud_conflicts WHERE id=?1", [id])?;
        tx.commit()?;
        drop(c);
        self.cloud_status()
    }
    /// Downloads and uploads receipt files. One unreadable or interrupted file
    /// never blocks the others; the first problem is reported after the pass.
    fn transfer_assets(&self, token: &str, owner: &str, generation: u64) -> Result<()> {
        let assets = self.cloud.rpc(token, "folio_assets", json!({}))?;
        let list = assets
            .as_array()
            .ok_or_else(|| fail("Invalid receipt asset manifest."))?;
        let mut first_error = None;
        for a in list {
            self.live(generation)?;
            if let Err(e) = self.download_asset(token, a) {
                first_error.get_or_insert(e);
            }
        }
        // The filesystem is scanned each cycle so images generated after metadata commit
        // are eventually uploaded even when no new database row is written.
        let receipts: Vec<String> = {
            let c = self.conn()?;
            let mut s = c.prepare("SELECT id FROM receipt_files")?;
            let rows = s.query_map([], |r| r.get(0))?;
            rows.collect::<std::result::Result<_, _>>()?
        };
        for id in receipts {
            self.live(generation)?;
            if let Err(e) = self.upload_receipt(token, owner, &id) {
                first_error.get_or_insert(e);
            }
        }
        match first_error {
            Some(e) => Err(e),
            None => self.verify_local_originals(),
        }
    }
    fn download_asset(&self, token: &str, a: &Value) -> Result<()> {
        let relative = a["relative_path"]
            .as_str()
            .ok_or_else(|| fail("Missing receipt path."))?;
        let receipt = a["receipt_id"]
            .as_str()
            .ok_or_else(|| fail("Missing receipt identity."))?;
        if uuid::Uuid::parse_str(receipt).is_err()
            || relative.rsplit('/').next().is_none_or(|name| {
                !safe_asset_name(name) || relative != format!("receipts/{receipt}/{name}")
            })
        {
            return Err(fail("Cloud receipt path does not match its identity."));
        }
        let exists: bool = self.conn()?.query_row(
            "SELECT EXISTS(SELECT 1 FROM receipt_files WHERE id=?1)",
            [receipt],
            |r| r.get(0),
        )?;
        if !exists {
            return Ok(());
        }
        let target = self.paths.resolve(relative)?;
        let hash = a["sha256"]
            .as_str()
            .ok_or_else(|| fail("Missing receipt checksum."))?;
        let size = a["size_bytes"]
            .as_u64()
            .ok_or_else(|| fail("Missing receipt size."))?;
        if size > FILE_LIMIT {
            return Err(fail("Cloud receipt exceeds the supported file limit."));
        }
        if target.exists() {
            if digest(&fs::read(&target)?) == hash {
                return self.mark_uploaded(relative, hash);
            }
            if relative
                .rsplit('/')
                .next()
                .is_some_and(|n| n.starts_with("original."))
            {
                return Err(fail("A local original receipt differs from its verified cloud copy. Preserve both files and repair this receipt before syncing."));
            }
            return Ok(());
        }
        let bucket = a["bucket_id"]
            .as_str()
            .ok_or_else(|| fail("Missing receipt bucket."))?;
        let object = a["object_path"]
            .as_str()
            .ok_or_else(|| fail("Missing receipt object."))?;
        if !["folio-receipts", "folio-import"].contains(&bucket) {
            return Err(fail("Unexpected receipt bucket."));
        }
        let bytes = read_limited(
            self.cloud.send(
                self.cloud
                    .object(Method::GET, bucket, object)?
                    .bearer_auth(token),
            )?,
            FILE_LIMIT,
        )?;
        // An interrupted transfer is short or fails its checksum; nothing is kept and it retries.
        if bytes.len() as u64 != size || digest(&bytes) != hash {
            return Err(AppError::new(
                Failure::Retry.code(),
                "A receipt download was interrupted or failed verification. It will retry.",
            ));
        }
        let parent = target
            .parent()
            .ok_or_else(|| fail("Invalid receipt destination."))?;
        fs::create_dir_all(parent)?;
        let staging = self
            .paths
            .resolve(&format!("cache/{}.part", uuid::Uuid::new_v4()))?;
        fs::write(&staging, &bytes)?;
        fs::rename(staging, &target)?;
        self.mark_uploaded(relative, hash)
    }
    fn upload_receipt(&self, token: &str, owner: &str, id: &str) -> Result<()> {
        uuid::Uuid::parse_str(id).map_err(|_| fail("Invalid local receipt identity."))?;
        let dir = self.paths.resolve(&format!("receipts/{id}"))?;
        if !dir.exists() {
            return Ok(());
        }
        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            if !entry.file_type()?.is_file() {
                continue;
            }
            let name = entry.file_name().to_string_lossy().to_string();
            if !safe_asset_name(&name) {
                continue;
            }
            let relative = format!("receipts/{id}/{name}");
            let bytes = fs::read(self.paths.resolve(&relative)?)?;
            if bytes.is_empty() || bytes.len() as u64 > FILE_LIMIT {
                return Err(fail(
                    "A receipt file is empty or exceeds the 25 MB cloud limit.",
                ));
            }
            let hash = digest(&bytes);
            let uploaded: bool = self.conn()?.query_row(
                "SELECT EXISTS(SELECT 1 FROM cloud_uploaded WHERE relative_path=?1 AND sha256=?2)",
                params![relative, hash],
                |r| r.get(0),
            )?;
            if uploaded {
                continue;
            }
            let ext = name.rsplit('.').next().unwrap_or("bin");
            // The server only accepts objects under the signed-in account's own folder.
            let object = format!("{owner}/{id}/{hash}.{ext}");
            let size = bytes.len();
            let sent = self.cloud.send(
                self.cloud
                    .object(Method::POST, "folio-receipts", &object)?
                    .bearer_auth(token)
                    .header("Content-Type", mime(&name))
                    .body(bytes),
            );
            if let Err(e) = sent {
                if e.code == Failure::Retry.code() || e.code == Failure::Auth.code() {
                    return Err(e);
                }
                // The name is its checksum, so "already exists" is success once the stored bytes match.
                let saved = read_limited(
                    self.cloud.send(
                        self.cloud
                            .object(Method::GET, "folio-receipts", &object)?
                            .bearer_auth(token),
                    )?,
                    FILE_LIMIT,
                )?;
                if digest(&saved) != hash {
                    return Err(fail("Receipt upload failed verification."));
                }
            }
            self.cloud.rpc(token,"folio_asset_register",json!({"receipt_id":id,"relative_path":relative,"object_path":object,"sha256":hash,"size_bytes":size}))?;
            self.mark_uploaded(&relative, &hash)?;
        }
        Ok(())
    }
    fn verify_local_originals(&self) -> Result<()> {
        let originals: Vec<(String, String, i64)> = {
            let c = self.conn()?;
            let mut q = c.prepare("SELECT relative_path,sha256,size_bytes FROM receipt_files")?;
            let rows = q.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?;
            rows.collect::<std::result::Result<_, _>>()?
        };
        for (relative, expected, size) in originals {
            let path = self.paths.resolve(&relative)?;
            if !path.is_file() {
                return Err(fail("A receipt original is still awaiting transfer from another device. Keep that device open and retry sync."));
            }
            let bytes = fs::read(path)?;
            if bytes.len() as i64 != size || digest(&bytes) != expected {
                return Err(fail("A receipt original failed integrity verification. Preserve the file and restore a verified copy before syncing."));
            }
        }
        Ok(())
    }
    fn mark_uploaded(&self, path: &str, hash: &str) -> Result<()> {
        self.conn()?.execute(
            "INSERT OR IGNORE INTO cloud_uploaded VALUES(?1,?2)",
            params![path, hash],
        )?;
        Ok(())
    }
}
fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn safe_asset_name(name: &str) -> bool {
    name == "preview.webp"
        || name == "display.jpg"
        || name == "rendition.jpg"
        || name == "original.jpg"
        || name == "original.png"
        || name == "original.pdf"
        || name == "original.heic"
        || name == "original.jpeg"
        || name == "original.webp"
        || name == "original.heif"
        || name == "view.jpg"
}
fn mime(name: &str) -> &'static str {
    match name.rsplit('.').next() {
        Some("jpg" | "jpeg") => "image/jpeg",
        Some("png") => "image/png",
        Some("webp") => "image/webp",
        Some("pdf") => "application/pdf",
        Some("heic" | "heif") => "image/heic",
        _ => "application/octet-stream",
    }
}
fn same_row(local: Option<&Value>, remote: &Value) -> bool {
    if remote["deleted"] == true {
        local.is_none()
    } else {
        local.is_some_and(|l| same_value(l, &remote["data"]))
    }
}
/// Numbers compare by value: PostgreSQL sends float 1.0 as `1`, SQLite renders `1.0`.
fn same_value(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Number(x), Value::Number(y)) if x.is_f64() || y.is_f64() => {
            x.as_f64() == y.as_f64()
        }
        (Value::Object(x), Value::Object(y)) => {
            x.len() == y.len()
                && x.iter()
                    .all(|(k, v)| y.get(k).is_some_and(|w| same_value(v, w)))
        }
        _ => a == b,
    }
}
/// Whether `remote` is exactly the change this device last sent for the record.
fn own_echo(c: &Connection, t: &str, key: &str, remote: &Value) -> Result<bool> {
    let sent: Option<String> = c
        .query_row(
            "SELECT sent FROM cloud_pending WHERE entity=?1 AND record_key=?2",
            params![t, key],
            |r| r.get(0),
        )
        .optional()?
        .flatten();
    let Some(sent) = sent.and_then(|s| serde_json::from_str::<Value>(&s).ok()) else {
        return Ok(false);
    };
    Ok(if remote["deleted"] == true {
        sent["operation"] == "delete"
    } else {
        sent["operation"] == "upsert" && same_value(&sent["data"], &remote["data"])
    })
}
fn record_conflict(
    c: &Connection,
    t: &str,
    key: &str,
    local: &Value,
    remote: &Value,
) -> Result<()> {
    c.execute("INSERT INTO cloud_conflicts(id,entity,record_key,local_data,remote_record) VALUES(?1,?2,?3,?4,?5) ON CONFLICT(entity,record_key) DO UPDATE SET local_data=excluded.local_data,remote_record=excluded.remote_record,choice=NULL",params![uuid::Uuid::new_v4().to_string(),t,key,serde_json::to_string(local)?,serde_json::to_string(remote)?])?;
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::security::secrets::SecretStore;
    use std::sync::Arc;
    struct Empty;
    impl SecretStore for Empty {
        fn get(&self) -> Result<Option<Zeroizing<String>>> {
            Ok(None)
        }
        fn set(&self, _: &str) -> Result<()> {
            Ok(())
        }
        fn delete(&self) -> Result<()> {
            Ok(())
        }
    }
    fn setup() -> (tempfile::TempDir, AppService) {
        let dir = tempfile::tempdir().unwrap();
        let s = AppService::open(dir.path().to_owned(), Arc::new(Empty)).unwrap();
        (dir, s)
    }
    #[test]
    fn mutations_capture_atomically_and_remote_does_not_echo() {
        let (_dir, s) = setup();
        let mut c = s.conn().unwrap();
        let tx = c.transaction().unwrap();
        tx.execute("INSERT INTO expense_claims(id,claim_number,title,status,currency,created_at,updated_at) VALUES('test','t','first','draft','MYR','now','now')",[]).unwrap();
        assert_eq!(
            tx.query_row(
                "SELECT count(*) FROM cloud_pending WHERE entity='expense_claims'",
                [],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
            1
        );
        tx.rollback().unwrap();
        assert_eq!(
            c.query_row(
                "SELECT count(*) FROM cloud_pending WHERE entity='expense_claims'",
                [],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
            0
        );
        let remote = json!({"entity":"expense_claims","key":{"id":"test"},"revision":1,"deleted":false,"data":{"id":"test","claim_number":"t","title":"remote","description":"","status":"draft","currency":"MYR","created_at":"now","updated_at":"now","version":1,"sync_state":"local_only"}});
        let tx = c.transaction().unwrap();
        tx.execute("UPDATE cloud_control SET applying=1", [])
            .unwrap();
        s.merge_remote(&tx, &remote).unwrap();
        tx.execute("UPDATE cloud_control SET applying=0", [])
            .unwrap();
        tx.commit().unwrap();
        assert_eq!(
            c.query_row(
                "SELECT count(*) FROM cloud_pending WHERE entity='expense_claims'",
                [],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
            0
        );
    }
    #[test]
    fn conflict_preserves_current_local_and_remote() {
        let (_dir, s) = setup();
        let c = s.conn().unwrap();
        c.execute("INSERT INTO expense_claims(id,claim_number,title,status,currency,created_at,updated_at) VALUES('test','t','local','draft','MYR','now','now')",[]).unwrap();
        let remote = json!({"entity":"expense_claims","key":{"id":"test"},"revision":2,"deleted":false,"data":{"id":"test","title":"remote"}});
        s.merge_remote(&c, &remote).unwrap();
        assert_eq!(
            c.query_row(
                "SELECT title FROM expense_claims WHERE id='test'",
                [],
                |r| r.get::<_, String>(0)
            )
            .unwrap(),
            "local"
        );
        assert_eq!(
            c.query_row("SELECT count(*) FROM cloud_conflicts", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            1
        );
    }
    #[test]
    fn fresh_device_discards_only_untouched_default_workspace() {
        let (_dir, s) = setup();
        let mut c = s.conn().unwrap();
        let id: String = c
            .query_row("SELECT id FROM workspaces", [], |r| r.get(0))
            .unwrap();
        let remote = json!({"entity":"workspaces","key":{"id":"remote"},"data":{"kind":"personal"},"deleted":false});
        let tx = c.transaction().unwrap();
        tx.execute("UPDATE cloud_control SET applying=1", [])
            .unwrap();
        s.prune_default_workspace(&tx, &[remote]).unwrap();
        assert_eq!(
            tx.query_row("SELECT count(*) FROM workspaces WHERE id=?1", [id], |r| r
                .get::<_, i64>(
                0
            ))
            .unwrap(),
            0
        );
        assert_eq!(
            tx.query_row("SELECT count(*) FROM categories", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            0
        );
        tx.rollback().unwrap();
        c.execute(
            "UPDATE workspaces SET name='My own workspace',version=2",
            [],
        )
        .unwrap();
        let tx = c.transaction().unwrap();
        tx.execute("UPDATE cloud_control SET applying=1", [])
            .unwrap();
        s.prune_default_workspace(&tx,&[json!({"entity":"workspaces","key":{"id":"remote"},"data":{"kind":"personal"},"deleted":false})]).unwrap();
        assert_eq!(
            tx.query_row("SELECT count(*) FROM workspaces", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            1
        );
    }
    #[test]
    fn legacy_expenses_keep_their_data_when_adopting_the_cloud_workspace() {
        let (_dir, s) = setup();
        let mut c = s.conn().unwrap();
        c.execute("INSERT INTO receipt_files(id,sha256,original_filename,mime_type,relative_path,size_bytes,created_at,updated_at) VALUES('legacy-receipt','legacy-hash','receipt.jpg','image/jpeg','receipts/legacy-receipt/original.jpg',1,'then','then')", []).unwrap();
        c.execute("INSERT INTO expenses(id,receipt_id,status,currency,created_at,updated_at) VALUES('legacy-expense','legacy-receipt','draft','MYR','then','then')", []).unwrap();
        let before = row(&c, "expenses", &json!({"id":"legacy-expense"})).unwrap();
        let tx = c.transaction().unwrap();
        tx.execute("UPDATE cloud_control SET applying=1", [])
            .unwrap();
        let remote = json!({"entity":"workspaces","key":{"id":"remote"},"data":{"kind":"personal"},"deleted":false});
        s.prune_default_workspace(&tx, &[remote]).unwrap();
        assert_eq!(
            tx.query_row("SELECT count(*) FROM workspaces", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            0
        );
        assert_eq!(
            row(&tx, "expenses", &json!({"id":"legacy-expense"})).unwrap(),
            before
        );
        assert_eq!(
            tx.query_row("SELECT count(*) FROM receipt_files", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            1
        );
        assert_eq!(
            tx.query_row(
                "SELECT count(*) FROM cloud_pending WHERE entity='expenses'",
                [],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
            1
        );
    }
    #[test]
    fn new_edit_survives_old_ack_and_composite_keys_match() {
        let (_dir, s) = setup();
        let c = s.conn().unwrap();
        c.execute("INSERT INTO expense_claims(id,claim_number,title,status,currency,created_at,updated_at) VALUES('test','t','first','draft','MYR','now','now')",[]).unwrap();
        let old: String = c
            .query_row(
                "SELECT change_id FROM cloud_pending WHERE entity='expense_claims'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(uuid::Uuid::parse_str(&old).unwrap().to_string(), old);
        c.execute(
            "UPDATE expense_claims SET title='newer' WHERE id='test'",
            [],
        )
        .unwrap();
        set_version(&c, "expense_claims", "{\"id\":\"test\"}", 5).unwrap();
        c.execute("DELETE FROM cloud_pending WHERE change_id=?1", [old])
            .unwrap();
        c.execute(
            "UPDATE cloud_pending SET base_revision=5 WHERE entity='expense_claims'",
            [],
        )
        .unwrap();
        let data: String = c
            .query_row(
                "SELECT data FROM cloud_pending WHERE entity='expense_claims'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(
            serde_json::from_str::<Value>(&data).unwrap()["title"],
            "newer"
        );
        c.execute("INSERT INTO exchange_rates VALUES('MYR','SGD','2026-11-05','2026-11-05','0.3','test','now')",[]).unwrap();
        let key: String = c
            .query_row(
                "SELECT record_key FROM cloud_pending WHERE entity='exchange_rates'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(
            key,
            canonical_key(
                &c,
                "exchange_rates",
                &json!({"requested_date":"2026-11-05","quote":"SGD","base":"MYR"})
            )
            .unwrap()
        );
    }
    #[test]
    fn staged_remote_resolution_waits_for_aggregate() {
        let (_dir, s) = setup();
        let c = s.conn().unwrap();
        c.execute("INSERT INTO expense_claims(id,claim_number,title,status,currency,created_at,updated_at) VALUES('test','t','local','draft','MYR','now','now')",[]).unwrap();
        let remote = json!({"entity":"expense_claims","key":{"id":"test"},"revision":2,"deleted":true,"data":{}});
        record_conflict(
            &c,
            "expense_claims",
            "{\"id\":\"test\"}",
            &json!({"title":"local"}),
            &remote,
        )
        .unwrap();
        let local = row(&c, "expense_claims", &json!({"id":"test"}))
            .unwrap()
            .unwrap();
        c.execute(
            "UPDATE cloud_conflicts SET choice='remote',local_data=?1",
            [serde_json::to_string(&local).unwrap()],
        )
        .unwrap();
        assert!(s.remote_conflict(&c, &remote).unwrap().is_none());
        assert!(row(&c, "expense_claims", &json!({"id":"test"}))
            .unwrap()
            .is_some());
        c.execute("UPDATE cloud_control SET applying=1", [])
            .unwrap();
        s.merge_remote(&c, &remote).unwrap();
        assert!(row(&c, "expense_claims", &json!({"id":"test"}))
            .unwrap()
            .is_none());
        assert_eq!(
            c.query_row(
                "SELECT count(*) FROM cloud_pending WHERE entity='expense_claims'",
                [],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
            0
        );
    }
    #[test]
    fn a_valid_access_token_is_reused_and_an_expired_one_is_not() {
        let (_dir, s) = setup();
        let owner = uuid::Uuid::new_v4().to_string();
        let mut cache = Some(Access {
            token: Zeroizing::new("cached".into()),
            until: Instant::now() + Duration::from_secs(60),
        });
        assert_eq!(
            s.cloud_access(&mut cache, &owner).unwrap().as_str(),
            "cached"
        );
        cache.as_mut().unwrap().until = Instant::now();
        // This test database has no stored session, so an expired token cannot be used.
        assert!(s.cloud_access(&mut cache, &owner).is_err());
        let fresh = Session::parse(
            json!({"access_token":"a","refresh_token":"r","expires_in":3600,"user":{"id":owner,"email":"a@example.com","app_metadata":{"providers":["email","google"]}}}),
        )
        .unwrap();
        assert!(fresh.cached().until > Instant::now() + Duration::from_secs(3000));
        assert_eq!(fresh.providers, ["email", "google"]);
        let unknown =
            Session::parse(json!({"access_token":"a","refresh_token":"r","user":{"id":owner}}))
                .unwrap();
        assert!(unknown.cached().until <= Instant::now());
        // A reply without credentials or a real account is never a session.
        assert!(Session::parse(json!({"access_token":"a","user":{"id":owner}})).is_err());
        assert!(Session::parse(
            json!({"access_token":"a","refresh_token":"r","user":{"id":"../x"}})
        )
        .is_err());
    }
    #[test]
    fn an_opened_email_link_still_signs_in_from_the_browser_address() {
        let opened = "http://localhost:3000/#access_token=aaa&expires_at=1&expires_in=3600&refresh_token=r3fr35h&token_type=bearer&type=signup";
        assert_eq!(
            redirect_session(opened).unwrap().map(|t| t.to_string()),
            Some("r3fr35h".to_string())
        );
        let expired = "http://localhost:3000/#error=access_denied&error_code=otp_expired&error_description=Email+link+is+invalid+or+has+expired";
        assert!(redirect_session(expired)
            .unwrap_err()
            .message
            .contains("Email link is invalid or has expired"));
        let unopened = "https://jvzaufwijwdqqhmhroet.supabase.co/auth/v1/verify?token=abc&type=signup&redirect_to=http://localhost:3000";
        assert!(redirect_session(unopened).unwrap().is_none());
        assert!(redirect_session("123456").unwrap().is_none());
    }
    #[test]
    fn a_workspace_belongs_to_one_account_and_backs_up_before_joining() {
        let (_dir, s) = setup();
        let (first, second) = (
            uuid::Uuid::new_v4().to_string(),
            uuid::Uuid::new_v4().to_string(),
        );
        assert_eq!(s.cloud_status().unwrap().state, "local");
        assert!(!s.has_user_data().unwrap());
        s.conn().unwrap().execute("INSERT INTO expense_claims(id,claim_number,title,status,currency,created_at,updated_at) VALUES('c','1','Mine','draft','MYR','now','now')",[]).unwrap();
        assert!(s.has_user_data().unwrap());
        assert!(s.bind_owner("not-an-account").is_err());
        s.bind_owner(&first).unwrap();
        s.bind_owner(&first).unwrap();
        assert!(s.bind_owner(&second).is_err());
        assert_eq!(s.cloud_owner().unwrap(), Some(first.clone()));
        let backups: Vec<_> = fs::read_dir(s.paths.root.join("database"))
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| {
                e.file_name()
                    .to_string_lossy()
                    .starts_with("before-account-")
            })
            .collect();
        assert_eq!(backups.len(), 1, "one snapshot from the first link only");
        let copy = Connection::open(backups[0].path()).unwrap();
        assert_eq!(
            copy.query_row("SELECT title FROM expense_claims", [], |r| r
                .get::<_, String>(0))
                .unwrap(),
            "Mine"
        );
        // Another identity can never store its session here.
        let other =
            Session::parse(json!({"access_token":"a","refresh_token":"r","user":{"id":second}}))
                .unwrap();
        assert!(s.accept_session(&other).is_err());
        assert!(!s.cloud_status().unwrap().signed_in);
    }
    #[test]
    fn retries_back_off_and_respect_the_server() {
        assert!((4..=6).contains(&retry_delay(Some(Failure::Retry), 0, None)));
        assert!((16..=24).contains(&retry_delay(Some(Failure::Retry), 2, None)));
        assert!(retry_delay(Some(Failure::Retry), 60, None) <= 1080);
        assert_eq!(retry_delay(Some(Failure::Quota), 0, Some(120)), 120);
        assert_eq!(retry_delay(Some(Failure::Quota), 0, Some(999_999)), 3600);
        assert_eq!(retry_delay(Some(Failure::Validation), 0, None), 900);
    }
    #[test]
    fn browser_sign_in_is_bound_to_this_app_and_used_once() {
        let (_dir, s) = setup();
        let (flow, url) = oauth_url(&s.cloud, "google").unwrap();
        let url = reqwest::Url::parse(&url).unwrap();
        let get = |k: &str| {
            url.query_pairs()
                .find(|(key, _)| key == k)
                .unwrap()
                .1
                .to_string()
        };
        assert_eq!(url.path(), "/auth/v1/authorize");
        assert_eq!(get("redirect_to"), "expenseapp://auth/callback");
        assert_eq!(get("code_challenge_method"), "s256");
        // The challenge is the SHA-256 of a verifier that never leaves the core.
        assert_eq!(
            get("code_challenge"),
            base64::engine::general_purpose::URL_SAFE_NO_PAD
                .encode(Sha256::digest(flow.verifier.as_bytes()))
        );
        assert!((43..=128).contains(&flow.verifier.len()));
        assert!(!url.as_str().contains(flow.verifier.as_str()));
        assert!(oauth_url(&s.cloud, "facebook").is_err());
        let message = |mut flow: Option<Pkce>, callback: &str| {
            oauth_session(&s.cloud, &mut flow, callback)
                .err()
                .unwrap()
                .message
        };
        // Receipt links and look-alike callbacks are not sign-in callbacks, and
        // they leave a sign-in that is in progress untouched.
        let mut waiting = Some(Pkce::new(false).0);
        assert!(oauth_session(&s.cloud, &mut waiting, "expenseapp://receipt/abc").is_err());
        assert!(waiting.is_some());
        assert!(message(None, "expenseapp://receipt/abc").contains("not a Folio sign-in"));
        assert!(message(None, "https://auth/callback?code=x").contains("not a Folio sign-in"));
        // Cancelling at the provider is reported, not treated as a session.
        assert!(message(
            None,
            "expenseapp://auth/callback?error=access_denied&error_description=User+cancelled"
        )
        .contains("User cancelled"));
        assert!(
            message(None, "expenseapp://auth/callback#error=server_error").contains("server_error")
        );
        // A second or stale callback has no flow left to redeem.
        assert!(message(None, "expenseapp://auth/callback?code=abc").contains("already finished"));
        let mut old = Pkce::new(false).0;
        old.started = Instant::now() - Duration::from_secs(301);
        assert!(message(Some(old), "expenseapp://auth/callback?code=abc").contains("expired"));
    }
    #[test]
    fn each_local_database_has_its_own_cloud_session_slot() {
        // A test driver or copy with another data folder must never use this session.
        let (_a, first) = setup();
        let (_b, second) = setup();
        let account = first.vault_id().unwrap();
        assert_eq!(account, first.vault_id().unwrap());
        assert_ne!(account, second.vault_id().unwrap());
    }
    #[test]
    fn whole_number_decimal_from_cloud_is_not_a_conflict() {
        // PostgreSQL sends float 1.0 as `1`; SQLite renders the same local value as `1.0`.
        let (_dir, s) = setup();
        let c = s.conn().unwrap();
        c.execute("INSERT INTO expenses(id,status,extraction_confidence,created_at,updated_at) VALUES('e','draft',1.0,'now','now')",[]).unwrap();
        let mut data = row(&c, "expenses", &json!({"id":"e"})).unwrap().unwrap();
        data["extraction_confidence"] = json!(1);
        let remote =
            json!({"entity":"expenses","key":{"id":"e"},"revision":1,"deleted":false,"data":data});
        assert!(s.remote_conflict(&c, &remote).unwrap().is_none());
        s.merge_remote(&c, &remote).unwrap();
        assert_eq!(
            c.query_row("SELECT count(*) FROM cloud_conflicts", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            0
        );
        data["extraction_confidence"] = json!(0.5);
        let changed =
            json!({"entity":"expenses","key":{"id":"e"},"revision":2,"deleted":false,"data":data});
        c.execute(
            "UPDATE expenses SET description='local edit' WHERE id='e'",
            [],
        )
        .unwrap();
        assert!(s.remote_conflict(&c, &changed).unwrap().is_some());
    }
    #[test]
    fn missing_or_corrupt_original_cannot_report_synced() {
        let (_dir, s) = setup();
        let id = uuid::Uuid::new_v4().to_string();
        let relative = format!("receipts/{id}/original.jpg");
        let bytes = b"test original";
        s.conn().unwrap().execute("INSERT INTO receipt_files(id,sha256,original_filename,mime_type,relative_path,size_bytes,created_at,updated_at) VALUES(?1,?2,'receipt.jpg','image/jpeg',?3,?4,'now','now')",params![id,digest(bytes),relative,bytes.len() as i64]).unwrap();
        assert!(s.verify_local_originals().is_err());
        let path = s.paths.resolve(&relative).unwrap();
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, bytes).unwrap();
        assert!(s.verify_local_originals().is_ok());
        fs::write(&path, b"corrupt").unwrap();
        assert!(s.verify_local_originals().is_err());
    }
    #[test]
    fn tombstone_and_bad_paths() {
        let (_d, t) = setup();
        assert!(t
            .cloud
            .object(Method::GET, "folio-receipts", "a/../b")
            .is_err());
        assert!(!safe_asset_name("../../password"));
        let (_dir, s) = setup();
        let c = s.conn().unwrap();
        let remote = json!({"entity":"expense_claims","key":{"id":"gone"},"revision":2,"deleted":true,"data":{}});
        s.merge_remote(&c, &remote).unwrap();
        assert_eq!(
            c.query_row(
                "SELECT revision FROM cloud_versions WHERE entity='expense_claims'",
                [],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
            2
        );
    }
}
