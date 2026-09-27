use super::AppService;
use crate::{
    domain::{
        claim::{Claim, ClaimDetail, ClaimStatus, ExportSnapshot},
        expense::{currency_exponent, ExpenseStatus},
        id, now,
    },
    error::{AppError, Result},
    repository::{
        self, claim_repository as claims, expense_repository as expenses,
        receipt_repository as receipts,
    },
};
use rusqlite::params;
impl AppService {
    pub fn claims(&self) -> Result<Vec<Claim>> {
        claims::all(&*self.conn()?)
    }
    pub fn claim(&self, id: &str) -> Result<ClaimDetail> {
        let db = self.conn()?;
        Ok(ClaimDetail {
            claim: claims::get(&db, id)?,
            expenses: expenses::all(&db)?
                .into_iter()
                .filter(|e| e.claim_id.as_deref() == Some(id))
                .collect(),
        })
    }
    pub fn create_claim(&self, title: String, currency: String) -> Result<Claim> {
        if title.trim().is_empty() || title.len() > 200 {
            return Err(AppError::invalid(
                "Enter a claim title of up to 200 characters.",
            ));
        }
        currency_exponent(&currency)?;
        let mut db = self.conn()?;
        let tx = db.transaction()?;
        let id = id();
        let time = now();
        let number = format!(
            "CL-{}-{}",
            chrono::Utc::now().format("%Y%m%d"),
            id[..8].to_uppercase()
        );
        tx.execute("INSERT INTO expense_claims(id,claim_number,title,currency,status,created_at,updated_at) VALUES(?1,?2,?3,?4,'draft',?5,?5)",params![id,number,title.trim(),currency,time])?;
        repository::audit(&tx, "claim.created", &id, serde_json::json!({}))?;
        tx.commit()?;
        claims::get(&db, &id)
    }
    pub fn edit_claim(
        &self,
        id: &str,
        version: i32,
        title: String,
        description: String,
    ) -> Result<Claim> {
        if title.trim().is_empty() || title.len() > 200 || description.len() > 4000 {
            return Err(AppError::invalid("Check the claim title and description."));
        }
        let mut db = self.conn()?;
        let tx = db.transaction()?;
        let claim = claims::get(&tx, id)?;
        if !matches!(claim.status, ClaimStatus::Draft) {
            return Err(AppError::invalid("Reopen the claim before editing it."));
        }
        if claim.version != version {
            return Err(AppError::new(
                "Conflict",
                "The claim has changed. Reload before saving.",
            ));
        }
        tx.execute("UPDATE expense_claims SET title=?1,description=?2,version=version+1,updated_at=?3,sync_state='local_only' WHERE id=?4",params![title.trim(),description,now(),id])?;
        repository::audit(&tx, "claim.edited", id, serde_json::json!({}))?;
        tx.commit()?;
        claims::get(&db, id)
    }
    pub fn set_claim_expense(
        &self,
        claim_id: &str,
        expense_id: &str,
        add: bool,
    ) -> Result<ClaimDetail> {
        let mut db = self.conn()?;
        let tx = db.transaction()?;
        let claim = claims::get(&tx, claim_id)?;
        if !matches!(claim.status, ClaimStatus::Draft) {
            return Err(AppError::invalid(
                "Reopen the claim to change its expenses.",
            ));
        }
        let e = expenses::get(&tx, expense_id)?;
        let time = now();
        if add {
            if e.status != ExpenseStatus::Ready
                || e.claim_id.is_some()
                || e.currency.as_deref() != Some(&claim.currency)
            {
                return Err(AppError::invalid(
                    "Only unclaimed, ready expenses in the claim's currency can be added.",
                ));
            }
            if claim.expense_count >= 250 {
                return Err(AppError::invalid("A claim can contain up to 250 expenses."));
            }
            tx.execute("INSERT INTO claim_expenses(claim_id,expense_id,created_at,updated_at) VALUES(?1,?2,?3,?3)",params![claim_id,expense_id,time])?;
        } else {
            tx.execute(
                "DELETE FROM claim_expenses WHERE claim_id=?1 AND expense_id=?2",
                params![claim_id, expense_id],
            )?;
        }
        tx.execute("UPDATE expense_claims SET version=version+1,updated_at=?1,sync_state='local_only' WHERE id=?2",params![time,claim_id])?;
        tx.execute("UPDATE expenses SET version=version+1,updated_at=?1,sync_state='local_only' WHERE id=?2",params![time,expense_id])?;
        repository::audit(
            &tx,
            if add {
                "expense.added_to_claim"
            } else {
                "expense.removed_from_claim"
            },
            expense_id,
            serde_json::json!({"claimId":claim_id}),
        )?;
        tx.commit()?;
        drop(db);
        self.claim(claim_id)
    }
    pub fn transition_claim(&self, id: &str, status: ClaimStatus) -> Result<Claim> {
        let mut db = self.conn()?;
        let tx = db.transaction()?;
        let claim = claims::get(&tx, id)?;
        if claim.status.as_str() == status.as_str() {
            return Ok(claim);
        }
        let members: Vec<_> = expenses::all(&tx)?
            .into_iter()
            .filter(|e| e.claim_id.as_deref() == Some(id))
            .collect();
        if matches!(status, ClaimStatus::Submitted)
            && (!matches!(claim.status, ClaimStatus::Draft)
                || members.is_empty()
                || members.iter().any(|e| e.status != ExpenseStatus::Ready))
        {
            return Err(AppError::invalid(
                "A draft claim needs at least one expense, and every expense must be ready.",
            ));
        }
        if matches!(status, ClaimStatus::Archived)
            && members
                .iter()
                .any(|e| !matches!(e.status, ExpenseStatus::Ready | ExpenseStatus::Submitted))
        {
            return Err(AppError::invalid(
                "Review every expense before archiving this claim.",
            ));
        }
        let active_jobs:i32=tx.query_row("SELECT COUNT(*) FROM jobs j JOIN claim_expenses ce ON ce.expense_id=j.entity_id WHERE ce.claim_id=?1 AND j.job_type='extract_receipt' AND j.status IN ('pending','running')",[id],|r|r.get(0))?;
        if active_jobs > 0 {
            return Err(AppError::invalid(
                "Wait for receipt extraction to finish before changing the claim status.",
            ));
        }
        let expense_status = match status {
            ClaimStatus::Draft => "ready",
            ClaimStatus::Submitted => "submitted",
            ClaimStatus::Archived => "archived",
        };
        let time = now();
        tx.execute("UPDATE expense_claims SET status=?1,version=version+1,updated_at=?2,sync_state='local_only' WHERE id=?3",params![status.as_str(),time,id])?;
        tx.execute("UPDATE expenses SET status=?1,version=version+1,updated_at=?2,sync_state='local_only' WHERE id IN (SELECT expense_id FROM claim_expenses WHERE claim_id=?3)",params![expense_status,time,id])?;
        repository::audit(
            &tx,
            "claim.status_changed",
            id,
            serde_json::json!({"from":claim.status,"to":status}),
        )?;
        tx.commit()?;
        claims::get(&db, id)
    }
    pub fn request_pdf(&self, id: &str) -> Result<String> {
        let mut db = self.conn()?;
        let tx = db.transaction()?;
        let claim = claims::get(&tx, id)?;
        let members: Vec<_> = expenses::all(&tx)?
            .into_iter()
            .filter(|e| e.claim_id.as_deref() == Some(id))
            .collect();
        if members.is_empty()
            || members.iter().any(|e| {
                !matches!(
                    e.status,
                    ExpenseStatus::Ready | ExpenseStatus::Submitted | ExpenseStatus::Archived
                )
            })
        {
            return Err(AppError::invalid(
                "Add at least one reviewed expense before exporting.",
            ));
        }
        let settings = Self::settings_in(&tx)?;
        let mut files = Vec::new();
        for e in &members {
            if let Some(ref id) = e.receipt_id {
                files.push(receipts::get(&tx, id)?);
            }
        }
        let snapshot = ExportSnapshot {
            claim,
            expenses: members,
            receipts: files,
            company: settings.company,
            employee: settings.employee,
            include_receipts: settings.include_receipts,
            generated_at: now(),
        };
        let active:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM jobs WHERE job_type='generate_pdf' AND entity_id=?1 AND status IN ('pending','running'))",[id],|r|r.get(0))?;
        if active {
            return Err(AppError::invalid(
                "A PDF is already being generated for this claim.",
            ));
        }
        let job = repository::enqueue(&tx, "generate_pdf", id, &serde_json::to_string(&snapshot)?)?;
        tx.commit()?;
        Ok(job)
    }
}
