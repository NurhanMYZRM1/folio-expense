use super::AppService;
use crate::{
    domain::{
        expense::{validate_values, Expense, ExpenseEdit, ExpenseStatus},
        extraction::{FieldMeta, FieldSource},
        id, now,
    },
    error::{AppError, Result},
    repository::{self, expense_repository as expenses},
};
use rusqlite::params;
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
    pub fn edit_expense(&self, edit: ExpenseEdit) -> Result<Expense> {
        validate_values(&edit)?;
        let mut db = self.conn()?;
        let tx = db.transaction()?;
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
        if let Some(ref claim_id) = e.claim_id {
            let currency: String = tx.query_row(
                "SELECT currency FROM expense_claims WHERE id=?1",
                [claim_id],
                |r| r.get(0),
            )?;
            if edit.currency.as_deref() != Some(currency.as_str()) {
                return Err(AppError::invalid(
                    "Remove this expense from its claim before changing currency.",
                ));
            }
        }
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
