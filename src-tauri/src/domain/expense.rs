use super::extraction::FieldMeta;
use crate::error::{AppError, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use ts_rs::TS;

pub const CATEGORIES: &[&str] = &[
    "Meals",
    "Transport",
    "Accommodation",
    "Fuel",
    "Parking",
    "Office Supplies",
    "Travel",
    "Entertainment",
    "Software",
    "Other",
];
pub const MAX_MONEY: i64 = 999_999_999_999;
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum ExpenseStatus {
    Draft,
    Extracting,
    NeedsReview,
    Ready,
    Submitted,
    Archived,
}
impl ExpenseStatus {
    pub fn can_transition(self, next: Self) -> bool {
        use ExpenseStatus::*;
        self == next
            || matches!(
                (self, next),
                (Draft, Extracting | NeedsReview | Ready | Archived)
                    | (Extracting, NeedsReview | Ready | Archived)
                    | (NeedsReview, Extracting | Ready | Archived)
                    | (Ready, Extracting | NeedsReview | Submitted | Archived)
                    | (Submitted, Ready | Archived)
                    | (Archived, Ready | NeedsReview)
            )
    }
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Draft => "draft",
            Self::Extracting => "extracting",
            Self::NeedsReview => "needs_review",
            Self::Ready => "ready",
            Self::Submitted => "submitted",
            Self::Archived => "archived",
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct Expense {
    pub id: String,
    pub receipt_id: Option<String>,
    pub occurred_at: Option<String>,
    pub merchant_name: Option<String>,
    #[ts(type = "number | null")]
    pub total_amount_minor: Option<i64>,
    #[ts(type = "number | null")]
    pub tax_amount_minor: Option<i64>,
    pub currency: Option<String>,
    pub category: String,
    pub description: String,
    pub status: ExpenseStatus,
    pub field_meta: BTreeMap<String, FieldMeta>,
    pub extraction_confidence: Option<f64>,
    pub created_at: String,
    pub updated_at: String,
    pub version: i32,
    pub sync_state: String,
    pub receipt_filename: Option<String>,
    pub claim_id: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ExpenseEdit {
    pub id: String,
    pub version: i32,
    pub occurred_at: Option<String>,
    pub merchant_name: Option<String>,
    #[ts(type = "number | null")]
    pub total_amount_minor: Option<i64>,
    #[ts(type = "number | null")]
    pub tax_amount_minor: Option<i64>,
    pub currency: Option<String>,
    pub category: String,
    pub description: String,
    pub mark_ready: bool,
}
pub fn currency_exponent(currency: &str) -> Result<u32> {
    match currency {
        "JPY" | "KRW" => Ok(0),
        "BHD" | "KWD" | "OMR" => Ok(3),
        "MYR" | "USD" | "SGD" | "EUR" | "GBP" | "AUD" | "CAD" | "CHF" | "CNY" | "HKD" | "INR"
        | "THB" | "IDR" => Ok(2),
        _ => Err(AppError::invalid("Select a supported currency.")),
    }
}
pub fn parse_money(input: &str, exponent: u32) -> Result<i64> {
    if exponent > 3 {
        return Err(AppError::invalid("Unsupported currency precision."));
    }
    let clean = input.trim().replace(',', "");
    let parts: Vec<_> = clean.split('.').collect();
    if parts.is_empty()
        || parts.len() > 2
        || parts[0].is_empty()
        || !parts[0].bytes().all(|c| c.is_ascii_digit())
    {
        return Err(AppError::invalid("Enter a positive amount."));
    }
    let fraction = parts.get(1).copied().unwrap_or("");
    if fraction.len() > exponent as usize || !fraction.bytes().all(|c| c.is_ascii_digit()) {
        return Err(AppError::invalid("Invalid decimal places."));
    }
    let whole = parts[0]
        .parse::<i64>()
        .map_err(|_| AppError::invalid("Amount is too large."))?;
    let frac = if fraction.is_empty() {
        0
    } else {
        fraction
            .parse::<i64>()
            .map_err(|_| AppError::invalid("Invalid amount."))?
    };
    let value = whole
        .checked_mul(10_i64.pow(exponent))
        .and_then(|v| v.checked_add(frac * 10_i64.pow(exponent - fraction.len() as u32)))
        .ok_or_else(|| AppError::invalid("Amount is too large."))?;
    if value > MAX_MONEY {
        return Err(AppError::invalid("Amount is too large."));
    }
    Ok(value)
}
pub fn validate_values(edit: &ExpenseEdit) -> Result<()> {
    if edit.merchant_name.as_ref().is_some_and(|v| v.len() > 300) || edit.description.len() > 4000 {
        return Err(AppError::invalid("Merchant or notes are too long."));
    }
    if !CATEGORIES.contains(&edit.category.as_str()) {
        return Err(AppError::invalid("Select a valid category."));
    }
    if let Some(ref d) = edit.occurred_at {
        if d.len() != 10 || chrono::NaiveDate::parse_from_str(d, "%Y-%m-%d").is_err() {
            return Err(AppError::invalid("Enter a valid date."));
        }
    }
    if let Some(ref c) = edit.currency {
        currency_exponent(c)?;
    }
    for v in [edit.total_amount_minor, edit.tax_amount_minor]
        .into_iter()
        .flatten()
    {
        if !(0..=MAX_MONEY).contains(&v) {
            return Err(AppError::invalid("Amount is outside the supported range."));
        }
    }
    if let (Some(tax), Some(total)) = (edit.tax_amount_minor, edit.total_amount_minor) {
        if tax > total {
            return Err(AppError::invalid("Tax cannot exceed the total."));
        }
    }
    if edit.mark_ready
        && (edit
            .merchant_name
            .as_ref()
            .is_none_or(|v| v.trim().is_empty())
            || edit.occurred_at.is_none()
            || edit.total_amount_minor.is_none()
            || edit.currency.is_none())
    {
        return Err(AppError::invalid(
            "Merchant, date, total and currency are required to mark an expense ready.",
        ));
    }
    Ok(())
}
