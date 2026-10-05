use crate::{
    domain::expense::{Expense, ExpenseStatus},
    error::{AppError, Result},
};
use rusqlite::{params, Connection, Row};
const SELECT:&str="SELECT e.id,e.receipt_id,e.occurred_at,e.merchant_name,e.total_amount_minor,e.tax_amount_minor,e.currency,e.category,e.description,e.status,e.field_meta,e.extraction_confidence,e.created_at,e.updated_at,e.version,e.sync_state,r.original_filename,ce.claim_id,e.original_currency,e.original_total_amount_minor,e.original_tax_amount_minor,e.exchange_rate,e.exchange_rate_date,e.premises FROM expenses e LEFT JOIN receipt_files r ON r.id=e.receipt_id LEFT JOIN claim_expenses ce ON ce.expense_id=e.id";
fn row(r: &Row) -> rusqlite::Result<Expense> {
    let status: String = r.get(9)?;
    let status = match status.as_str() {
        "draft" => ExpenseStatus::Draft,
        "extracting" => ExpenseStatus::Extracting,
        "needs_review" => ExpenseStatus::NeedsReview,
        "ready" => ExpenseStatus::Ready,
        "submitted" => ExpenseStatus::Submitted,
        "archived" => ExpenseStatus::Archived,
        _ => return Err(rusqlite::Error::InvalidQuery),
    };
    let meta: String = r.get(10)?;
    let field_meta = serde_json::from_str(&meta).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(10, rusqlite::types::Type::Text, Box::new(e))
    })?;
    Ok(Expense {
        id: r.get(0)?,
        receipt_id: r.get(1)?,
        occurred_at: r.get(2)?,
        merchant_name: r.get(3)?,
        total_amount_minor: r.get(4)?,
        tax_amount_minor: r.get(5)?,
        currency: r.get(6)?,
        category: r.get(7)?,
        description: r.get(8)?,
        status,
        field_meta,
        extraction_confidence: r.get(11)?,
        created_at: r.get(12)?,
        updated_at: r.get(13)?,
        version: r.get(14)?,
        sync_state: r.get(15)?,
        receipt_filename: r.get(16)?,
        claim_id: r.get(17)?,
        original_currency: r.get(18)?,
        original_total_amount_minor: r.get(19)?,
        original_tax_amount_minor: r.get(20)?,
        exchange_rate: r.get(21)?,
        exchange_rate_date: r.get(22)?,
        premises: r.get(23)?,
    })
}
pub fn all(db: &Connection) -> Result<Vec<Expense>> {
    let mut st = db.prepare(&format!(
        "{SELECT} ORDER BY COALESCE(e.occurred_at,e.created_at) DESC,e.created_at DESC"
    ))?;
    let rows = st
        .query_map([], row)?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}
pub fn get(db: &Connection, id: &str) -> Result<Expense> {
    match db.query_row(&format!("{SELECT} WHERE e.id=?1"), [id], row) {
        Ok(v) => Ok(v),
        Err(rusqlite::Error::QueryReturnedNoRows) => Err(AppError::not_found()),
        Err(e) => Err(e.into()),
    }
}
pub fn save(db: &Connection, e: &Expense) -> Result<()> {
    let previous = get(db, &e.id)?;
    if !previous.status.can_transition(e.status) {
        return Err(AppError::invalid(
            "This expense status change is not allowed.",
        ));
    }
    let count=db.execute("UPDATE expenses SET occurred_at=?1,merchant_name=?2,total_amount_minor=?3,tax_amount_minor=?4,currency=?5,category=?6,description=?7,status=?8,field_meta=?9,extraction_confidence=?10,updated_at=?11,original_currency=?14,original_total_amount_minor=?15,original_tax_amount_minor=?16,exchange_rate=?17,exchange_rate_date=?18,premises=?19,version=version+1,sync_state='local_only' WHERE id=?12 AND version=?13",params![e.occurred_at,e.merchant_name,e.total_amount_minor,e.tax_amount_minor,e.currency,e.category,e.description,e.status.as_str(),serde_json::to_string(&e.field_meta)?,e.extraction_confidence,e.updated_at,e.id,e.version,e.original_currency,e.original_total_amount_minor,e.original_tax_amount_minor,e.exchange_rate,e.exchange_rate_date,e.premises])?;
    if count != 1 {
        return Err(AppError::new(
            "Conflict",
            "This expense changed in the background. Reload it before saving.",
        ));
    }
    Ok(())
}
