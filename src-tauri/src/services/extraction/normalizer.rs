use crate::{
    domain::{
        expense::{validate_values, ExpenseEdit, CATEGORIES},
        extraction::Extraction,
    },
    error::{AppError, Result},
};
pub fn validate(result: &Extraction) -> Result<()> {
    let invalid = || {
        AppError::new(
            "InvalidExtraction",
            "Extraction returned invalid values. The receipt is safe and can be entered manually.",
        )
    };
    if result.confidence.len() != 6
        || [
            "merchantName",
            "date",
            "total",
            "tax",
            "currency",
            "category",
        ]
        .iter()
        .any(|k| !result.confidence.contains_key(*k))
        || result
            .confidence
            .values()
            .any(|v| !v.is_finite() || !(0.0..=1.0).contains(v))
    {
        return Err(invalid());
    }
    if result
        .suggested_category
        .as_ref()
        .is_some_and(|s| !CATEGORIES.contains(&s.as_str()))
    {
        return Err(invalid());
    }
    let edit = ExpenseEdit {
        id: String::new(),
        version: 1,
        occurred_at: result.date.clone(),
        merchant_name: result.merchant_name.clone(),
        total_amount_minor: result.total_amount_minor,
        tax_amount_minor: result.tax_amount_minor,
        currency: result.currency.clone(),
        category: result
            .suggested_category
            .clone()
            .unwrap_or_else(|| "Other".into()),
        description: String::new(),
        mark_ready: false,
    };
    validate_values(&edit).map_err(|_| invalid())
}
