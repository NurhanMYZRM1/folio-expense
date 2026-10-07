use super::{expense::Expense, receipt::ReceiptFile};
use serde::{Deserialize, Serialize};
use ts_rs::TS;
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum ClaimStatus {
    Draft,
    Submitted,
    Archived,
}
impl ClaimStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Draft => "draft",
            Self::Submitted => "submitted",
            Self::Archived => "archived",
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct Claim {
    pub id: String,
    pub claim_number: String,
    pub title: String,
    pub description: String,
    pub status: ClaimStatus,
    pub currency: String,
    #[ts(type = "number")]
    pub total_amount_minor: i64,
    pub expense_count: i32,
    pub created_at: String,
    pub updated_at: String,
    pub version: i32,
    pub sync_state: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ClaimDetail {
    pub claim: Claim,
    pub expenses: Vec<Expense>,
    /// When the claim's contents (expense values, membership, title) last
    /// changed. Status changes don't count. A report generated before this
    /// moment no longer matches the claim.
    pub content_changed_at: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ExportSnapshot {
    pub claim: Claim,
    pub expenses: Vec<Expense>,
    pub receipts: Vec<ReceiptFile>,
    pub company: String,
    pub employee: String,
    pub include_receipts: bool,
    pub generated_at: String,
}
