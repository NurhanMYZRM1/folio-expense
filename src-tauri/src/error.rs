use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, Clone, Serialize, Deserialize, TS, thiserror::Error)]
#[serde(rename_all = "camelCase")]
#[error("{message}")]
pub struct AppError {
    pub code: String,
    pub message: String,
    pub existing_expense_id: Option<String>,
}
pub type Result<T> = std::result::Result<T, AppError>;
impl AppError {
    pub fn new(code: &str, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            existing_expense_id: None,
        }
    }
    pub fn invalid(message: impl Into<String>) -> Self {
        Self::new("ValidationError", message)
    }
    pub fn not_found() -> Self {
        Self::new("NotFound", "This local record could not be found.")
    }
}
impl From<rusqlite::Error> for AppError {
    fn from(_: rusqlite::Error) -> Self {
        Self::new("DatabaseError", "The local database could not complete this operation. Your saved data has not been removed.")
    }
}
impl From<std::io::Error> for AppError {
    fn from(_: std::io::Error) -> Self {
        Self::new(
            "StorageError",
            "Unable to read or write local storage. Check available space and folder permissions.",
        )
    }
}
impl From<serde_json::Error> for AppError {
    fn from(_: serde_json::Error) -> Self {
        Self::new("InvalidData", "The data could not be decoded safely.")
    }
}
