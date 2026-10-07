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
        let content_changed_at = db.query_row(
            "SELECT MAX(created_at) FROM audit_events WHERE \
             (event_type IN ('expense.edited','expense.converted') \
                AND entity_id IN (SELECT expense_id FROM claim_expenses WHERE claim_id=?1)) \
             OR (event_type IN ('expense.added_to_claim','expense.removed_from_claim') \
                AND json_extract(details,'$.claimId')=?1) \
             OR (event_type='claim.edited' AND entity_id=?1)",
            [id],
            |r| r.get(0),
        )?;
        Ok(ClaimDetail {
            claim: claims::get(&db, id)?,
            expenses: expenses::all(&db)?
                .into_iter()
                .filter(|e| e.claim_id.as_deref() == Some(id))
                .collect(),
            content_changed_at,
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
        if add {
            return self.add_claim_expenses(claim_id, vec![expense_id.to_string()]);
        }
        let mut db = self.conn()?;
        let tx = db.transaction()?;
        let claim = claims::get(&tx, claim_id)?;
        if !matches!(claim.status, ClaimStatus::Draft) {
            return Err(AppError::invalid(
                "Reopen the claim to change its expenses.",
            ));
        }
        expenses::get(&tx, expense_id)?;
        let time = now();
        let removed = tx.execute(
            "DELETE FROM claim_expenses WHERE claim_id=?1 AND expense_id=?2",
            params![claim_id, expense_id],
        )?;
        if removed == 0 {
            return Err(AppError::invalid("This expense is not in the claim."));
        }
        tx.execute("UPDATE expense_claims SET version=version+1,updated_at=?1,sync_state='local_only' WHERE id=?2",params![time,claim_id])?;
        tx.execute("UPDATE expenses SET version=version+1,updated_at=?1,sync_state='local_only' WHERE id=?2",params![time,expense_id])?;
        repository::audit(
            &tx,
            "expense.removed_from_claim",
            expense_id,
            serde_json::json!({"claimId":claim_id}),
        )?;
        tx.commit()?;
        drop(db);
        self.claim(claim_id)
    }
    /// Adds imported expenses to a draft claim, all or nothing. Expenses that
    /// still need review may be added; the claim cannot be submitted until
    /// every expense is reviewed and ready.
    pub fn add_claim_expenses(
        &self,
        claim_id: &str,
        expense_ids: Vec<String>,
    ) -> Result<ClaimDetail> {
        let expense_ids: Vec<String> = expense_ids
            .into_iter()
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .collect();
        if expense_ids.is_empty() {
            return Err(AppError::invalid("Choose at least one expense to add."));
        }
        let mut db = self.conn()?;
        let tx = db.transaction()?;
        let claim = claims::get(&tx, claim_id)?;
        if !matches!(claim.status, ClaimStatus::Draft) {
            return Err(AppError::invalid(
                "Reopen the claim to change its expenses.",
            ));
        }
        if claim.expense_count as usize + expense_ids.len() > 250 {
            return Err(AppError::invalid("A claim can contain up to 250 expenses."));
        }
        let time = now();
        for expense_id in &expense_ids {
            let e = expenses::get(&tx, expense_id)?;
            let name = e.merchant_name.as_deref().unwrap_or("This expense");
            if e.claim_id.is_some() {
                return Err(AppError::invalid(format!("{name} is already in a claim.")));
            }
            if !matches!(e.status, ExpenseStatus::Ready | ExpenseStatus::NeedsReview) {
                return Err(AppError::invalid(format!(
                    "{name} is still being read. Add it when processing finishes."
                )));
            }
            if e.currency.as_deref() != Some(&claim.currency) {
                return Err(AppError::invalid(format!(
                    "{name} is in {} and this claim is in {}. Only expenses in the claim's currency can be added.",
                    e.currency.as_deref().unwrap_or("no currency"),
                    claim.currency
                )));
            }
            tx.execute("INSERT INTO claim_expenses(claim_id,expense_id,created_at,updated_at) VALUES(?1,?2,?3,?3)",params![claim_id,expense_id,time])?;
            tx.execute("UPDATE expenses SET version=version+1,updated_at=?1,sync_state='local_only' WHERE id=?2",params![time,expense_id])?;
            repository::audit(
                &tx,
                "expense.added_to_claim",
                expense_id,
                serde_json::json!({"claimId":claim_id}),
            )?;
        }
        tx.execute("UPDATE expense_claims SET version=version+1,updated_at=?1,sync_state='local_only' WHERE id=?2",params![time,claim_id])?;
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
                "Add at least one expense, and review every expense in this claim, before exporting.",
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
