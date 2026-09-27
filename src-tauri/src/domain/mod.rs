pub mod claim;
pub mod expense;
pub mod extraction;
pub mod receipt;
pub mod settings;
use chrono::{SecondsFormat, Utc};
pub fn now() -> String {
    Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true)
}
pub fn id() -> String {
    uuid::Uuid::new_v4().to_string()
}
