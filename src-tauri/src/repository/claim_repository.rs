use crate::{
    domain::claim::{Claim, ClaimStatus},
    error::{AppError, Result},
};
use rusqlite::{Connection, Row};
const SELECT:&str="SELECT c.id,c.claim_number,c.title,c.description,c.status,c.currency,COALESCE(SUM(e.total_amount_minor),0),COUNT(e.id),c.created_at,c.updated_at,c.version,c.sync_state FROM expense_claims c LEFT JOIN claim_expenses ce ON ce.claim_id=c.id LEFT JOIN expenses e ON e.id=ce.expense_id";
fn row(r: &Row) -> rusqlite::Result<Claim> {
    let status: String = r.get(4)?;
    Ok(Claim {
        id: r.get(0)?,
        claim_number: r.get(1)?,
        title: r.get(2)?,
        description: r.get(3)?,
        status: match status.as_str() {
            "draft" => ClaimStatus::Draft,
            "submitted" => ClaimStatus::Submitted,
            "archived" => ClaimStatus::Archived,
            _ => return Err(rusqlite::Error::InvalidQuery),
        },
        currency: r.get(5)?,
        total_amount_minor: r.get(6)?,
        expense_count: r.get(7)?,
        created_at: r.get(8)?,
        updated_at: r.get(9)?,
        version: r.get(10)?,
        sync_state: r.get(11)?,
    })
}
pub fn all(db: &Connection) -> Result<Vec<Claim>> {
    let mut st = db.prepare(&format!(
        "{SELECT} GROUP BY c.id ORDER BY c.created_at DESC"
    ))?;
    let rows = st
        .query_map([], row)?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}
pub fn get(db: &Connection, id: &str) -> Result<Claim> {
    match db.query_row(&format!("{SELECT} WHERE c.id=?1 GROUP BY c.id"), [id], row) {
        Ok(c) => Ok(c),
        Err(rusqlite::Error::QueryReturnedNoRows) => Err(AppError::not_found()),
        Err(e) => Err(e.into()),
    }
}
