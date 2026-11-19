//! Business records are isolated by workspace. Money is integer minor units;
//! quantities are integer thousandths. Issued invoices are immutable snapshots.
use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BusinessProfile {
    pub workspace_id: String,
    pub legal_name: String,
    pub registration_number: String,
    pub tin: String,
    pub sst_registration_number: String,
    pub address: String,
    pub email: String,
    pub phone: String,
    pub invoice_prefix: String,
    pub version: i32,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CustomerInput {
    pub id: Option<String>,
    pub version: Option<i32>,
    pub workspace_id: String,
    pub name: String,
    pub registration_number: String,
    pub tin: String,
    pub address: String,
    pub email: String,
    pub phone: String,
    pub archived: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct Customer {
    pub id: String,
    pub workspace_id: String,
    pub name: String,
    pub registration_number: String,
    pub tin: String,
    pub address: String,
    pub email: String,
    pub phone: String,
    pub archived: bool,
    pub version: i32,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct InventoryItemInput {
    pub id: Option<String>,
    pub version: Option<i32>,
    pub workspace_id: String,
    pub sku: String,
    pub name: String,
    pub unit: String,
    pub currency: String,
    #[ts(type = "number")]
    pub unit_price_minor: i64,
    pub archived: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct InventoryItem {
    pub id: String,
    pub workspace_id: String,
    pub sku: String,
    pub name: String,
    pub unit: String,
    pub currency: String,
    #[ts(type = "number")]
    pub unit_price_minor: i64,
    #[ts(type = "number")]
    pub stock_milli: i64,
    pub archived: bool,
    pub version: i32,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StockAdjustmentInput {
    /// A UUID created once per user action; reuse it on retries.
    pub request_id: String,
    pub workspace_id: String,
    pub item_id: String,
    #[ts(type = "number")]
    pub quantity_milli: i64,
    pub occurred_on: String,
    pub reason: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct StockMovement {
    pub id: String,
    pub workspace_id: String,
    pub item_id: String,
    #[ts(type = "number")]
    pub quantity_milli: i64,
    pub occurred_on: String,
    pub reason: String,
    pub invoice_id: Option<String>,
    pub created_at: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct InvoiceLineInput {
    pub inventory_item_id: Option<String>,
    pub description: String,
    /// 1000 means one unit; multiply price then round half-up per line.
    #[ts(type = "number")]
    pub quantity_milli: i64,
    #[ts(type = "number")]
    pub unit_price_minor: i64,
    #[ts(type = "number")]
    pub discount_minor: i64,
    /// User-selected tax rate, never inferred from the description.
    pub tax_basis_points: i32,
    /// MyInvois classification code; local preparation only.
    pub classification_code: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct InvoiceLine {
    #[serde(flatten)]
    #[ts(flatten)]
    pub input: InvoiceLineInput,
    #[ts(type = "number")]
    pub subtotal_minor: i64,
    #[ts(type = "number")]
    pub tax_minor: i64,
    #[ts(type = "number")]
    pub total_minor: i64,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum InvoiceStatus {
    Draft,
    Issued,
    Paid,
    Void,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct InvoiceInput {
    pub id: Option<String>,
    pub version: Option<i32>,
    pub workspace_id: String,
    pub customer_id: String,
    pub currency: String,
    pub issued_on: String,
    pub due_on: String,
    pub note: String,
    pub lines: Vec<InvoiceLineInput>,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct Invoice {
    pub id: String,
    pub workspace_id: String,
    /// Assigned atomically on issue; never reused after voiding.
    pub number: Option<String>,
    pub customer_id: String,
    pub currency: String,
    pub issued_on: String,
    pub due_on: String,
    pub note: String,
    pub lines: Vec<InvoiceLine>,
    #[ts(type = "number")]
    pub subtotal_minor: i64,
    #[ts(type = "number")]
    pub tax_minor: i64,
    #[ts(type = "number")]
    pub total_minor: i64,
    #[ts(type = "number")]
    pub paid_minor: i64,
    pub status: InvoiceStatus,
    pub supplier: Option<BusinessProfile>,
    pub customer: Customer,
    pub version: i32,
    pub created_at: String,
    pub updated_at: String,
    pub payments: Vec<InvoicePayment>,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct InvoicePaymentInput {
    pub request_id: String,
    pub workspace_id: String,
    pub invoice_id: String,
    pub account_id: String,
    #[ts(type = "number")]
    pub amount_minor: i64,
    pub paid_on: String,
    pub note: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct InvoicePayment {
    pub id: String,
    pub account_id: String,
    pub transaction_id: String,
    #[ts(type = "number")]
    pub amount_minor: i64,
    pub paid_on: String,
    pub note: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct BusinessCurrencyTotal {
    pub currency: String,
    #[ts(type = "number")]
    pub invoiced_minor: i64,
    #[ts(type = "number")]
    pub paid_minor: i64,
    #[ts(type = "number")]
    pub outstanding_minor: i64,
    #[ts(type = "number")]
    pub overdue_minor: i64,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct BusinessReport {
    pub workspace_id: String,
    pub as_of: String,
    pub by_currency: Vec<BusinessCurrencyTotal>,
    pub draft_count: u32,
    pub issued_count: u32,
}
