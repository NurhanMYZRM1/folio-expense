//! The validated Supabase project configuration, one bounded HTTPS client and
//! the failure classes shared by authentication and synchronization.
use crate::{
    error::{AppError, Result},
    security::secrets::SecretStore,
};
use base64::Engine;
use reqwest::blocking::{Client, RequestBuilder, Response};
use serde_json::Value;
use std::{
    io::Read,
    sync::{Arc, Mutex},
    time::Duration,
};

const PRODUCTION_URL: &str = "https://jvzaufwijwdqqhmhroet.supabase.co";
// A publishable key identifies the app; the signed-in user's JWT authorizes requests.
const PRODUCTION_KEY: &str = "sb_publishable_fYEtE4iFTkI-_Nq_bETdAg_p2LlfRHO";
/// Where Supabase returns the browser after Google or Apple sign-in.
pub const CALLBACK: &str = "expenseapp://auth/callback";
const RESPONSE_LIMIT: u64 = 16 * 1024 * 1024;
pub(crate) const FILE_LIMIT: u64 = 26_214_400;

#[derive(Clone, Debug)]
pub struct CloudConfig {
    pub environment: String,
    pub url: String,
    pub key: String,
    pub callback: String,
}
impl CloudConfig {
    /// Release builds use the project they were compiled for. Debug and test
    /// builds may point at a development stack through the environment.
    pub fn current() -> Result<Self> {
        #[allow(unused_mut)]
        let mut config = Self {
            environment: option_env!("FOLIO_CLOUD_ENV")
                .unwrap_or("production")
                .into(),
            url: option_env!("FOLIO_CLOUD_URL")
                .unwrap_or(PRODUCTION_URL)
                .into(),
            key: option_env!("FOLIO_CLOUD_KEY")
                .unwrap_or(PRODUCTION_KEY)
                .into(),
            callback: CALLBACK.into(),
        };
        #[cfg(any(debug_assertions, feature = "test-support"))]
        if let (Ok(url), Ok(key)) = (
            std::env::var("FOLIO_CLOUD_URL"),
            std::env::var("FOLIO_CLOUD_KEY"),
        ) {
            config.environment =
                std::env::var("FOLIO_CLOUD_ENV").unwrap_or_else(|_| "development".into());
            config.url = url;
            config.key = key;
        }
        config.validate()?;
        Ok(config)
    }
    pub fn validate(&self) -> Result<()> {
        let bad = |why: &str| AppError::new("CloudConfig", format!("Cloud configuration: {why}"));
        let url = reqwest::Url::parse(&self.url).map_err(|_| bad("the project URL is invalid."))?;
        let host = url.host_str().unwrap_or_default();
        let local = matches!(host, "127.0.0.1" | "localhost");
        let production = self.environment == "production";
        if !url.username().is_empty()
            || url.password().is_some()
            || url.query().is_some()
            || url.path() != "/"
        {
            return Err(bad("the project URL must be a bare origin."));
        }
        if url.scheme() != "https" && !(url.scheme() == "http" && local && !production) {
            return Err(bad("the project URL must use HTTPS."));
        }
        if production != (self.url == PRODUCTION_URL) {
            return Err(bad(
                "production and development projects must not be mixed.",
            ));
        }
        // Legacy anon keys are JWTs; anything carrying another role is privileged.
        let role = self.key.split('.').nth(1).and_then(|payload| {
            let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
                .decode(payload)
                .ok()?;
            serde_json::from_slice::<Value>(&bytes).ok()?["role"]
                .as_str()
                .map(str::to_owned)
        });
        if !(self.key.starts_with("sb_publishable_") || role.as_deref() == Some("anon")) {
            return Err(bad(
                "only a publishable key may be used; secret and service keys never belong in the app.",
            ));
        }
        let callback = reqwest::Url::parse(&self.callback)
            .map_err(|_| bad("the sign-in callback is invalid."))?;
        if matches!(callback.scheme(), "http" | "https") {
            return Err(bad("the sign-in callback must open this app."));
        }
        Ok(())
    }
    pub fn host(&self) -> String {
        reqwest::Url::parse(&self.url)
            .ok()
            .and_then(|u| u.host_str().map(str::to_owned))
            .unwrap_or_default()
    }
    pub fn key_kind(&self) -> &'static str {
        if self.key.starts_with("sb_publishable_") {
            "publishable"
        } else {
            "legacy anon"
        }
    }
}

/// How a failed cloud request should be handled.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Failure {
    /// Network trouble or a server fault: retry with backoff.
    Retry,
    /// The access token was refused: refresh once, then ask to sign in.
    Auth,
    /// The account may not do this: surface it, do not hammer.
    Permission,
    /// The request itself was rejected: surface it, keep the pending work.
    Validation,
    /// Rate limited: wait as long as the server asks.
    Quota,
    /// The server holds a different version.
    Conflict,
}
impl Failure {
    pub fn code(self) -> &'static str {
        match self {
            Self::Retry => "CloudRetry",
            Self::Auth => "CloudAuth",
            Self::Permission => "CloudPermission",
            Self::Validation => "CloudValidation",
            Self::Quota => "CloudQuota",
            Self::Conflict => "CloudConflict",
        }
    }
    pub fn from_code(code: &str) -> Option<Self> {
        [
            Self::Retry,
            Self::Auth,
            Self::Permission,
            Self::Validation,
            Self::Quota,
            Self::Conflict,
        ]
        .into_iter()
        .find(|f| f.code() == code)
    }
    pub fn from_status(status: u16) -> Self {
        match status {
            401 => Self::Auth,
            403 => Self::Permission,
            409 => Self::Conflict,
            429 => Self::Quota,
            408 | 425 | 500..=599 => Self::Retry,
            _ => Self::Validation,
        }
    }
    fn message(self) -> &'static str {
        match self {
            Self::Retry => "The cloud service is unavailable. Local changes are saved and will retry.",
            Self::Auth => "Cloud access expired or was refused. Sign in again.",
            Self::Permission => "This account is not allowed to do that. Nothing was removed from this device.",
            Self::Validation => "The cloud service rejected this request. Pending changes remain saved on this device.",
            Self::Quota => "The cloud service asked Folio to slow down. Sync will resume shortly.",
            Self::Conflict => "The cloud already holds a different version. Nothing was overwritten.",
        }
    }
}

/// Facts about the most recent failed request that do not fit in an `AppError`.
#[derive(Clone, Debug, Default)]
pub struct FailureDetail {
    pub retry_after: Option<u64>,
    pub request_id: Option<String>,
    /// Supabase Auth's machine-readable reason, such as `refresh_token_not_found`.
    pub error_code: Option<String>,
}

pub type VaultFactory = Arc<dyn Fn(&str) -> Result<Box<dyn SecretStore>> + Send + Sync>;

pub struct Transport {
    pub config: CloudConfig,
    client: Client,
    vaults: VaultFactory,
    last: Mutex<FailureDetail>,
}
impl Transport {
    /// Sessions live in the operating system's credential store.
    pub fn new(config: CloudConfig) -> Result<Self> {
        Self::with_vaults(
            config,
            Arc::new(|account| {
                keyring::Entry::new("com.folio.expenses.cloud", account)
                    .map(|entry| Box::new(entry) as Box<dyn SecretStore>)
                    .map_err(|_| {
                        AppError::new(
                            "CloudSync",
                            "Secure cloud credential storage is unavailable.",
                        )
                    })
            }),
        )
    }
    pub fn with_vaults(config: CloudConfig, vaults: VaultFactory) -> Result<Self> {
        config.validate()?;
        let client = Client::builder()
            .connect_timeout(Duration::from_secs(10))
            .timeout(Duration::from_secs(30))
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(|_| {
                AppError::new(
                    "CloudSync",
                    "Cannot initialize the secure cloud connection.",
                )
            })?;
        Ok(Self {
            config,
            client,
            vaults,
            last: Mutex::default(),
        })
    }
    /// The credential slot for one local database. Other environments never share
    /// a slot with production, so a development server is never shown its sessions.
    pub(crate) fn vault(&self, id: &str) -> Result<Box<dyn SecretStore>> {
        let account = if self.config.environment == "production" {
            format!("supabase-refresh-v1-{id}")
        } else {
            format!(
                "supabase-refresh-v1-{}-{}-{id}",
                self.config.environment,
                self.config.host()
            )
        };
        (self.vaults)(&account)
    }
    pub(crate) fn request(&self, method: reqwest::Method, path: &str) -> RequestBuilder {
        self.client
            .request(method, format!("{}{path}", self.config.url))
            .header("apikey", &self.config.key)
    }
    pub(crate) fn object(
        &self,
        method: reqwest::Method,
        bucket: &str,
        path: &str,
    ) -> Result<RequestBuilder> {
        let invalid = || AppError::new("CloudSync", "Invalid cloud object path.");
        let mut url =
            reqwest::Url::parse(&format!("{}/storage/v1/object/{bucket}/", self.config.url))
                .map_err(|_| invalid())?;
        {
            let mut segments = url.path_segments_mut().map_err(|_| invalid())?;
            segments.pop_if_empty();
            for s in path.split('/') {
                if s.is_empty() || s == "." || s == ".." || s.contains('\\') {
                    return Err(invalid());
                }
                segments.push(s);
            }
        }
        Ok(self
            .client
            .request(method, url)
            .header("apikey", &self.config.key)
            // Receipts are up to 25 MB; allow a slow mobile connection to finish.
            .timeout(Duration::from_secs(120)))
    }
    /// Sends a request and classifies any failure. Server text is trimmed and
    /// never includes credentials; raw bodies are not kept.
    pub(crate) fn send(&self, request: RequestBuilder) -> Result<Response> {
        let mut detail = FailureDetail::default();
        let outcome = match request.send() {
            Err(_) => Err(AppError::new(
                Failure::Retry.code(),
                "Cloud connection unavailable. Local changes are saved and will retry.",
            )),
            Ok(r) if r.status().is_success() => Ok(r),
            Ok(r) => {
                let failure = Failure::from_status(r.status().as_u16());
                let header = |name: &str| {
                    r.headers()
                        .get(name)
                        .and_then(|v| v.to_str().ok())
                        .map(|v| v.chars().take(80).collect::<String>())
                };
                detail.request_id = header("sb-request-id").or_else(|| header("x-request-id"));
                detail.retry_after = header("retry-after").and_then(|v| v.parse().ok());
                let mut body = Vec::new();
                let _ = r.take(4096).read_to_end(&mut body);
                let json: Option<Value> = serde_json::from_slice(&body).ok();
                detail.error_code = json
                    .as_ref()
                    .and_then(|v| v["error_code"].as_str().or_else(|| v["error"].as_str()))
                    .map(str::to_owned);
                // Supabase Auth reports `msg`, PostgREST `message`.
                let reason = json.as_ref().and_then(|v| {
                    ["msg", "message", "error_description"]
                        .iter()
                        .find_map(|k| v[*k].as_str())
                        .map(|s| s.chars().take(300).collect::<String>())
                });
                Err(AppError::new(
                    failure.code(),
                    match reason {
                        Some(why) => format!("{} Supabase said: {why}", failure.message()),
                        None => failure.message().to_string(),
                    },
                ))
            }
        };
        if outcome.is_err() {
            *self.last.lock().unwrap_or_else(|e| e.into_inner()) = detail;
        }
        outcome
    }
    pub(crate) fn json(&self, request: RequestBuilder) -> Result<Value> {
        let bytes = read_limited(self.send(request)?, RESPONSE_LIMIT)?;
        serde_json::from_slice(&bytes).map_err(|_| interrupted())
    }
    pub(crate) fn rpc(&self, token: &str, name: &str, args: Value) -> Result<Value> {
        self.json(
            self.request(reqwest::Method::POST, &format!("/rest/v1/rpc/{name}"))
                .bearer_auth(token)
                .json(&args),
        )
    }
    pub(crate) fn last_failure(&self) -> FailureDetail {
        self.last.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }
}

fn interrupted() -> AppError {
    AppError::new(
        Failure::Retry.code(),
        "The cloud response was interrupted. Local changes are saved and will retry.",
    )
}
/// Reads a whole body, treating a dropped connection as retryable.
pub(crate) fn read_limited(response: Response, limit: u64) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    response
        .take(limit + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| interrupted())?;
    if bytes.len() as u64 > limit {
        return Err(AppError::new(
            Failure::Validation.code(),
            "Cloud response exceeded its safe size limit.",
        ));
    }
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn config(environment: &str, url: &str, key: &str) -> CloudConfig {
        CloudConfig {
            environment: environment.into(),
            url: url.into(),
            key: key.into(),
            callback: CALLBACK.into(),
        }
    }
    #[test]
    fn only_a_publishable_key_and_a_matching_project_are_accepted() {
        assert!(CloudConfig::current().is_ok());
        let dev = "http://127.0.0.1:54321";
        assert!(config("development", dev, "sb_publishable_test")
            .validate()
            .is_ok());
        // {"role":"anon"} and {"role":"service_role"} as unsigned JWT payloads.
        assert!(config("development", dev, "x.eyJyb2xlIjoiYW5vbiJ9.y")
            .validate()
            .is_ok());
        for key in ["sb_secret_abc", "x.eyJyb2xlIjoic2VydmljZV9yb2xlIn0.y", ""] {
            assert!(config("development", dev, key).validate().is_err());
        }
        // Plain HTTP is for a local stack only, and never called production.
        assert!(
            config("development", "http://example.com", "sb_publishable_x")
                .validate()
                .is_err()
        );
        assert!(config("production", dev, "sb_publishable_x")
            .validate()
            .is_err());
        assert!(config("development", PRODUCTION_URL, "sb_publishable_x")
            .validate()
            .is_err());
        assert!(config(
            "development",
            "https://user:pw@example.com",
            "sb_publishable_x"
        )
        .validate()
        .is_err());
    }
    #[test]
    fn failures_are_classified_by_what_to_do_next() {
        assert_eq!(Failure::from_status(503), Failure::Retry);
        assert_eq!(Failure::from_status(401), Failure::Auth);
        assert_eq!(Failure::from_status(403), Failure::Permission);
        assert_eq!(Failure::from_status(429), Failure::Quota);
        assert_eq!(Failure::from_status(409), Failure::Conflict);
        assert_eq!(Failure::from_status(400), Failure::Validation);
        assert_eq!(Failure::from_code("CloudQuota"), Some(Failure::Quota));
        assert_eq!(Failure::from_code("StorageError"), None);
    }
}
