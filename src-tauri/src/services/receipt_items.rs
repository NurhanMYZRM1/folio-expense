use super::AppService;
use crate::{
    domain::{
        expense::{currency_exponent, Expense, ExpenseStatus, MAX_MONEY},
        extraction::{Extraction, FieldSource},
        now,
        receipt_items::*,
    },
    error::{AppError, Result},
    repository::expense_repository,
};
use rusqlite::{params, Connection, OptionalExtension};

fn bounded(value: i64, signed: bool) -> Result<()> {
    if value > MAX_MONEY || value < if signed { -MAX_MONEY } else { 0 } {
        return Err(AppError::invalid(
            "A receipt item amount is outside the supported range.",
        ));
    }
    Ok(())
}
pub(crate) fn validate_items(items: &[ReceiptItem], a: &ReceiptItemAdjustments) -> Result<()> {
    if items.len() > 200 {
        return Err(AppError::invalid(
            "A receipt can contain up to 200 line items.",
        ));
    }
    for item in items {
        if item.description.trim().is_empty()
            || item.description.len() > 500
            || item.description.chars().any(|c| c == '\0')
            || !item.confidence.is_finite()
            || !(0.0..=1.0).contains(&item.confidence)
        {
            return Err(AppError::invalid(
                "Check the line descriptions and confidence values.",
            ));
        }
        bounded(item.amount_minor, false)?;
        if let Some(unit) = item.unit_price_minor {
            bounded(unit, false)?;
        }
        if item
            .quantity_milli
            .is_some_and(|qty| !(1..=999_999_999).contains(&qty))
        {
            return Err(AppError::invalid("Check the item quantity."));
        }
    }
    for v in [a.tax_minor, a.discount_minor, a.service_charge_minor] {
        bounded(v, false)?;
    }
    bounded(a.rounding_minor, true)?;
    Ok(())
}
fn reconcile(
    e: &Expense,
    currency: &str,
    items: &[ReceiptItem],
    a: &ReceiptItemAdjustments,
) -> Result<ReceiptItemsReconciliation> {
    currency_exponent(currency)?;
    validate_items(items, a)?;
    let (receipt_currency, total, _) = e.receipt_amounts();
    let subtotal: i128 = items.iter().map(|i| i128::from(i.amount_minor)).sum();
    let calculated = subtotal
        + if a.tax_included {
            0
        } else {
            i128::from(a.tax_minor)
        }
        + i128::from(a.service_charge_minor)
        - i128::from(a.discount_minor)
        + i128::from(a.rounding_minor);
    let safe = |value: i128| {
        i64::try_from(value)
            .ok()
            .filter(|v| (-MAX_MONEY..=MAX_MONEY).contains(v))
            .ok_or_else(|| AppError::invalid("Receipt line totals exceed the supported range."))
    };
    let subtotal = safe(subtotal)?;
    let calculated = safe(calculated)?;
    let matching = receipt_currency.as_deref() == Some(currency);
    let difference = total.filter(|_| matching).map(|total| calculated - total);
    let mut warnings = vec![];
    if !matching {
        warnings.push("The line-item currency differs from the original receipt. Convert claim amounts separately; do not convert receipt lines.".into());
    }
    if total.is_none() {
        warnings.push("Add the receipt total before confirming line items.".into());
    }
    if items.is_empty() {
        warnings.push("No line items have been entered.".into());
    }
    if difference.is_some_and(|v| v != 0) {
        warnings.push("Line items and adjustments do not equal the original receipt total. Check tax inclusion, service charge, discounts and rounding.".into());
    }
    for (i, item) in items.iter().enumerate() {
        if let (Some(q), Some(p)) = (item.quantity_milli, item.unit_price_minor) {
            let expected = (i128::from(q) * i128::from(p) + 500) / 1000;
            if expected != i128::from(item.amount_minor) {
                warnings.push(format!("Line {} quantity × price differs from its printed net amount; check discounts and OCR.",i+1));
            }
        }
    }
    Ok(ReceiptItemsReconciliation {
        items_subtotal_minor: subtotal,
        calculated_total_minor: calculated,
        receipt_total_minor: total,
        difference_minor: difference,
        balanced: matching && difference == Some(0) && !items.is_empty(),
        warnings,
    })
}
fn stored(db: &Connection, expense: &str) -> Result<Option<ReceiptItemsReview>> {
    db.query_row(
        "SELECT data FROM receipt_item_reviews WHERE expense_id=?1",
        [expense],
        |r| r.get::<_, String>(0),
    )
    .optional()?
    .map(|s| serde_json::from_str(&s).map_err(Into::into))
    .transpose()
}
fn write(db: &Connection, review: &ReceiptItemsReview) -> Result<()> {
    db.execute("INSERT INTO receipt_item_reviews(expense_id,data) VALUES(?1,?2) ON CONFLICT(expense_id) DO UPDATE SET data=excluded.data",params![review.expense_id,serde_json::to_string(review)?])?;
    Ok(())
}
/// Called inside the extraction transaction after the expense values are merged.
/// Manual drafts and reviews are preserved on every subsequent OCR retry.
pub(crate) fn capture_extracted_items(
    db: &Connection,
    e: &Expense,
    result: &Extraction,
    source: FieldSource,
) -> Result<()> {
    if result.line_items.is_empty() {
        return Ok(());
    }
    let Some(currency) = result.currency.as_deref() else {
        return Ok(());
    };
    let old = stored(db, &e.id)?;
    if old
        .as_ref()
        .is_some_and(|r| r.source == FieldSource::Manual)
    {
        return Ok(());
    }
    let adjustments =
        result
            .line_item_adjustments
            .clone()
            .unwrap_or_else(|| ReceiptItemAdjustments {
                tax_minor: result.tax_amount_minor.unwrap_or(0),
                ..Default::default()
            });
    let reconciliation = reconcile(e, currency, &result.line_items, &adjustments)?;
    let review = ReceiptItemsReview {
        expense_id: e.id.clone(),
        version: old.map_or(1, |r| r.version + 1),
        expense_version: e.version,
        currency: currency.into(),
        items: result.line_items.clone(),
        adjustments,
        reviewed: false,
        source,
        updated_at: now(),
        reconciliation,
    };
    write(db, &review)
}
impl AppService {
    pub fn get_receipt_items(&self, expense_id: &str) -> Result<ReceiptItemsReview> {
        let db = self.conn()?;
        let e = expense_repository::get(&db, expense_id)?;
        let mut review = if let Some(review) = stored(&db, expense_id)? {
            review
        } else {
            let (currency, _, tax) = e.receipt_amounts();
            let currency = currency.unwrap_or(Self::settings_in(&db)?.default_currency);
            let adjustments = ReceiptItemAdjustments {
                tax_minor: tax.unwrap_or(0),
                ..Default::default()
            };
            ReceiptItemsReview {
                expense_id: e.id.clone(),
                version: 0,
                expense_version: e.version,
                currency: currency.clone(),
                items: vec![],
                adjustments: adjustments.clone(),
                reviewed: false,
                source: FieldSource::Manual,
                updated_at: now(),
                reconciliation: reconcile(&e, &currency, &[], &adjustments)?,
            }
        };
        review.expense_version = e.version;
        review.reconciliation =
            reconcile(&e, &review.currency, &review.items, &review.adjustments)?;
        // A later expense edit can invalidate a previously balanced review.
        review.reviewed &= review.reconciliation.balanced;
        Ok(review)
    }
    pub fn save_receipt_items(&self, input: ReceiptItemsInput) -> Result<ReceiptItemsReview> {
        let mut db = self.conn()?;
        let tx = db.transaction()?;
        let e = expense_repository::get(&tx, &input.expense_id)?;
        if matches!(e.status, ExpenseStatus::Submitted | ExpenseStatus::Archived) {
            return Err(AppError::invalid(
                "Reopen the claim before editing its receipt line items.",
            ));
        }
        let old = stored(&tx, &input.expense_id)?;
        if input.expense_version != e.version
            || input.version != old.as_ref().map_or(0, |r| r.version)
        {
            return Err(AppError::new(
                "Conflict",
                "This receipt or its line items changed. Reload before saving.",
            ));
        }
        let reconciliation = reconcile(&e, &input.currency, &input.items, &input.adjustments)?;
        if input.reviewed && !reconciliation.balanced {
            return Err(AppError::invalid(
                "Reconcile line items with the original receipt total before confirming review.",
            ));
        }
        let review = ReceiptItemsReview {
            expense_id: e.id,
            version: input.version + 1,
            expense_version: e.version,
            currency: input.currency,
            items: input.items,
            adjustments: input.adjustments,
            reviewed: input.reviewed,
            source: FieldSource::Manual,
            updated_at: now(),
            reconciliation,
        };
        write(&tx, &review)?;
        tx.commit()?;
        Ok(review)
    }
}
