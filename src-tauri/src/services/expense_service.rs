use super::AppService;
use crate::{
    domain::{
        expense::{validate_values, Expense, ExpenseEdit, ExpenseStatus},
        extraction::{FieldMeta, FieldSource},
        id, now,
    },
    error::{AppError, Result},
    repository::{self, expense_repository as expenses, receipt_repository},
};
use rusqlite::params;
use std::fs;
impl AppService {
    pub fn expenses(&self) -> Result<Vec<Expense>> {
        expenses::all(&*self.conn()?)
    }
    pub fn expense(&self, id: &str) -> Result<Expense> {
        expenses::get(&*self.conn()?, id)
    }
    pub fn create_expense(&self) -> Result<Expense> {
        let mut db = self.conn()?;
        let tx = db.transaction()?;
        let id = id();
        let time = now();
        tx.execute(
            "INSERT INTO expenses(id,status,created_at,updated_at) VALUES(?1,'draft',?2,?2)",
            params![id, time],
        )?;
        repository::audit(
            &tx,
            "expense.created",
            &id,
            serde_json::json!({"manual":true}),
        )?;
        tx.commit()?;
        expenses::get(&db, &id)
    }
    /// `edit` carries the receipt's own currency and amounts; a foreign
    /// receipt is converted to the home currency from them.
    pub fn edit_expense(&self, edit: ExpenseEdit) -> Result<Expense> {
        validate_values(&edit)?;
        // Download the exchange rate (if any is needed) before taking the database lock.
        self.prefetch_for(edit.currency.as_deref(), edit.occurred_at.as_deref());
        let mut db = self.conn()?;
        let tx = db.transaction()?;
        let settings = Self::settings_in(&tx)?;
        let mut e = expenses::get(&tx, &edit.id)?;
        if e.version != edit.version {
            return Err(AppError::new(
                "Conflict",
                "This expense changed in the background. Reload before saving your changes.",
            ));
        }
        if matches!(e.status, ExpenseStatus::Submitted | ExpenseStatus::Archived) {
            return Err(AppError::invalid(
                "Reopen the claim before editing this expense.",
            ));
        }
        let before = e.clone();
        e.clear_conversion();
        let mut changed = Vec::new();
        macro_rules! update {
            ($field:ident,$key:literal) => {
                if e.$field != edit.$field {
                    e.$field = edit.$field.clone();
                    e.field_meta.insert(
                        $key.into(),
                        FieldMeta {
                            confidence: 1.,
                            source: FieldSource::Manual,
                        },
                    );
                    changed.push($key);
                }
            };
        }
        update!(merchant_name, "merchantName");
        update!(occurred_at, "date");
        update!(total_amount_minor, "total");
        update!(tax_amount_minor, "tax");
        update!(currency, "currency");
        update!(category, "category");
        update!(description, "description");
        Self::reconvert_in_tx(&tx, &mut e, &before, &settings)?;
        Self::check_claim_currency(&tx, &e)?;
        if edit.mark_ready {
            e.status = ExpenseStatus::Ready;
        } else if e.status == ExpenseStatus::Ready {
            e.status = ExpenseStatus::NeedsReview;
        }
        e.updated_at = now();
        expenses::save(&tx, &e)?;
        if let Some(ref claim_id) = e.claim_id {
            tx.execute("UPDATE expense_claims SET version=version+1,updated_at=?1,sync_state='local_only' WHERE id=?2",params![e.updated_at,claim_id])?;
        }
        repository::audit(
            &tx,
            "expense.edited",
            &e.id,
            serde_json::json!({"fields":changed,"status":e.status}),
        )?;
        tx.commit()?;
        expenses::get(&db, &e.id)
    }
    /// Permanently deletes expenses and their stored receipt files, all or
    /// nothing. Refused while a receipt is being processed or while an expense
    /// belongs to a submitted/archived claim. Removes expenses from draft claims.
    pub fn delete_expenses(&self, ids: Vec<String>) -> Result<u32> {
        let ids: Vec<String> = ids
            .into_iter()
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .collect();
        if ids.is_empty() || ids.len() > 5000 {
            return Err(AppError::invalid(
                "Choose between 1 and 5000 expenses to delete.",
            ));
        }
        let mut db = self.conn()?;
        let tx = db.transaction()?;
        let mut files = Vec::new();
        for id in &ids {
            let e = expenses::get(&tx, id)?;
            let name = e.merchant_name.as_deref().unwrap_or("This expense");
            if matches!(e.status, ExpenseStatus::Submitted | ExpenseStatus::Archived) {
                return Err(AppError::invalid(format!(
                    "{name} belongs to a submitted or archived claim. Reopen the claim before deleting it."
                )));
            }
            let busy: bool = tx.query_row(
                "SELECT EXISTS(SELECT 1 FROM jobs WHERE (entity_id=?1 AND status='running') OR (job_type='generate_pdf' AND entity_id=?2 AND status IN ('pending','running')))",
                params![id, e.claim_id],
                |r| r.get(0),
            )?;
            if busy {
                return Err(AppError::invalid(format!(
                    "{name} is still being processed. Try again in a moment."
                )));
            }
            tx.execute(
                "DELETE FROM jobs WHERE entity_id=?1 AND job_type IN ('extract_receipt','generate_thumbnail')",
                [id],
            )?;
            if let Some(ref claim_id) = e.claim_id {
                tx.execute("DELETE FROM claim_expenses WHERE expense_id=?1", [id])?;
                tx.execute(
                    "UPDATE expense_claims SET version=version+1,updated_at=?1,sync_state='local_only' WHERE id=?2",
                    params![now(), claim_id],
                )?;
            }
            tx.execute("DELETE FROM extraction_runs WHERE expense_id=?1", [id])?;
            tx.execute("DELETE FROM expenses WHERE id=?1", [id])?;
            if let Some(ref receipt_id) = e.receipt_id {
                let receipt = receipt_repository::get(&tx, receipt_id)?;
                tx.execute("DELETE FROM receipt_files WHERE id=?1", [receipt_id])?;
                files.push((receipt.id, receipt.relative_path));
            }
            repository::audit(
                &tx,
                "expense.deleted",
                id,
                serde_json::json!({"receiptId":e.receipt_id,"claimId":e.claim_id}),
            )?;
        }
        tx.commit()?;
        drop(db);
        // Files go only after the database no longer refers to them. Anything
        // left behind (e.g. a file open elsewhere) is cleaned up at next start.
        for (receipt_id, original) in files {
            for relative in [original, format!("receipts/{receipt_id}/preview.webp")] {
                if let Ok(path) = self.paths.resolve(&relative) {
                    let _ = fs::remove_file(&path);
                }
            }
            if let Ok(dir) = self.paths.resolve(&format!("receipts/{receipt_id}")) {
                let _ = fs::remove_dir(dir);
            }
        }
        Ok(ids.len() as u32)
    }
    /// An expense in a claim must stay in the claim's currency.
    pub(crate) fn check_claim_currency(db: &rusqlite::Connection, e: &Expense) -> Result<()> {
        let Some(ref claim_id) = e.claim_id else {
            return Ok(());
        };
        let currency: String = db.query_row(
            "SELECT currency FROM expense_claims WHERE id=?1",
            [claim_id],
            |r| r.get(0),
        )?;
        if e.currency.as_deref() == Some(currency.as_str()) {
            return Ok(());
        }
        Err(AppError::invalid(format!(
            "This expense is in a {currency} claim, but its amount is in {} and could not be converted to {currency} (no exchange rate yet; check your internet connection). Remove it from the claim or try again when online.",
            e.currency.as_deref().unwrap_or("no currency")
        )))
    }
    pub fn queue_extraction(&self, id: &str) -> Result<()> {
        let mut db = self.conn()?;
        let tx = db.transaction()?;
        let expense = expenses::get(&tx, id)?;
        if expense.receipt_id.is_none() {
            return Err(AppError::invalid("This expense has no receipt."));
        }
        if matches!(
            expense.status,
            ExpenseStatus::Submitted | ExpenseStatus::Archived
        ) {
            return Err(AppError::invalid(
                "Reopen the claim before extracting again.",
            ));
        }
        let active:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM jobs WHERE entity_id=?1 AND job_type='extract_receipt' AND status IN ('pending','running'))",[id],|r|r.get(0))?;
        if !active {
            repository::enqueue(&tx, "extract_receipt", id, "{}")?;
        }
        tx.commit()?;
        Ok(())
    }
}
