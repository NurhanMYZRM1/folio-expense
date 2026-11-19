//! Line-item review uses the receipt's original currency, never converted claim
//! amounts. Extracted candidates remain drafts until a person confirms them.
use crate::domain::extraction::FieldSource;
use serde::{Deserialize, Serialize};
use ts_rs::TS;
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReceiptItem {
    pub description: String,
    #[ts(type = "number | null")]
    pub quantity_milli: Option<i64>,
    #[ts(type = "number | null")]
    pub unit_price_minor: Option<i64>,
    /// Printed net line amount. Quantity/price are optional evidence only.
    #[ts(type = "number")]
    pub amount_minor: i64,
    pub confidence: f64,
}
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReceiptItemAdjustments {
    /// True when line amounts already contain the receipt's stated tax.
    pub tax_included: bool,
    #[ts(type = "number")]
    pub tax_minor: i64,
    /// Receipt-level discount only; line-level discounts are already in amount.
    #[ts(type = "number")]
    pub discount_minor: i64,
    #[ts(type = "number")]
    pub service_charge_minor: i64,
    #[ts(type = "number")]
    pub rounding_minor: i64,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ReceiptItemsReconciliation {
    #[ts(type = "number")]
    pub items_subtotal_minor: i64,
    #[ts(type = "number")]
    pub calculated_total_minor: i64,
    #[ts(type = "number | null")]
    pub receipt_total_minor: Option<i64>,
    /// Calculated total minus original receipt total; null if unknown.
    #[ts(type = "number | null")]
    pub difference_minor: Option<i64>,
    pub balanced: bool,
    pub warnings: Vec<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ReceiptItemsReview {
    pub expense_id: String,
    pub version: i32,
    /// The current expense version; pass back when saving to detect edits.
    pub expense_version: i32,
    pub currency: String,
    pub items: Vec<ReceiptItem>,
    pub adjustments: ReceiptItemAdjustments,
    pub reviewed: bool,
    pub source: FieldSource,
    pub updated_at: String,
    pub reconciliation: ReceiptItemsReconciliation,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReceiptItemsInput {
    pub expense_id: String,
    /// 0 creates a review when no extracted draft exists.
    pub version: i32,
    pub expense_version: i32,
    pub currency: String,
    pub items: Vec<ReceiptItem>,
    pub adjustments: ReceiptItemAdjustments,
    pub reviewed: bool,
}
