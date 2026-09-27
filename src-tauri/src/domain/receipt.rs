use serde::{Deserialize, Serialize};
use ts_rs::TS;
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ReceiptFile {
    pub id: String,
    pub sha256: String,
    pub original_filename: String,
    pub mime_type: String,
    pub relative_path: String,
    #[ts(type = "number")]
    pub size_bytes: i64,
    pub created_at: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ReceiptContent {
    pub receipt: ReceiptFile,
    pub base64: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ImportOutcome {
    pub filename: String,
    pub expense_id: Option<String>,
    pub error: Option<crate::error::AppError>,
}
