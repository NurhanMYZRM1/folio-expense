use super::*;
use crate::services::csv_export::CsvExport;
impl AppService {
    pub fn parse_quick_add(&self, w: &str, input: &str, today: &str) -> Result<QuickAddDraft> {
        let today_date = date(today)?;
        text(input, 2000, "a quick entry")?;
        let db = self.conn()?;
        let w = workspace(&db, w)?;
        let lower = input.to_lowercase();
        let kind = if ["received", "income", "salary", "earned", "gaji"]
            .iter()
            .any(|s| lower.contains(s))
        {
            TransactionKind::Income
        } else {
            TransactionKind::Expense
        };
        let words: Vec<_> = input.split_whitespace().collect();
        // Resolve the currency before interpreting any number, including "1500 JPY".
        let mut currency = w.base_currency;
        for word in &words {
            let token = word.trim_matches(|c: char| c == ',' || c == ';');
            let upper = token.to_uppercase();
            if currency_exponent(&upper).is_ok() {
                currency = upper.clone();
            }
            if upper == "RM"
                || upper
                    .strip_prefix("RM")
                    .is_some_and(|s| s.starts_with(|c: char| c.is_ascii_digit()))
            {
                currency = "MYR".into();
            }
        }
        let mut amount = None;
        let mut candidates = 0;
        let mut uncertain = vec!["account".into(), "category".into(), "payee".into()];
        let mut explicit_kind = false;
        for word in &words {
            let token = word.trim_matches(|c: char| c == ',' || c == ';');
            let stripped = token.trim_start_matches("RM").trim_start_matches("rm");
            if let Ok(v) =
                crate::domain::expense::parse_money(stripped, currency_exponent(&currency)?)
            {
                if v > 0 {
                    candidates += 1;
                    if amount.is_none() {
                        amount = Some(v);
                    }
                }
            }
        }
        if candidates > 1 {
            uncertain.push("amount".into());
        }
        if amount.is_none() {
            uncertain.push("amount".into());
        }
        for marker in [
            "spent", "paid", "bought", "received", "income", "salary", "earned", "gaji", "belanja",
            "bayar",
        ] {
            if lower.contains(marker) {
                explicit_kind = true;
            }
        }
        if !explicit_kind {
            uncertain.push("kind".into());
        }
        let mut occurred_on = today.into();
        if lower.contains("yesterday") || lower.contains("semalam") {
            occurred_on = (today_date - Duration::days(1)).to_string();
        } else {
            for word in &words {
                if date(word).is_ok() {
                    occurred_on = (*word).into();
                }
            }
        }
        Ok(QuickAddDraft {
            text: input.into(),
            kind,
            amount_minor: amount,
            currency: Some(currency),
            occurred_on,
            category_id: None,
            account_id: None,
            payee: None,
            note: input.into(),
            uncertain,
        })
    }
    pub fn suggestions(&self, w: &str) -> Result<Vec<TransactionSuggestion>> {
        let db = self.conn()?;
        workspace(&db, w)?;
        query(&db,"SELECT id,workspace_id,kind,amount_minor,currency,occurred_on,payee,app_label,status,transaction_id,created_at FROM transaction_suggestions WHERE workspace_id=?1 AND status='pending' ORDER BY created_at DESC",[w])
    }
    pub fn accept_suggestion(&self, id: &str, input: TransactionInput) -> Result<Transaction> {
        let mut db = self.conn()?;
        let tx = db.transaction()?;
        let s:TransactionSuggestion=one(&tx,"SELECT id,workspace_id,kind,amount_minor,currency,occurred_on,payee,app_label,status,transaction_id,created_at FROM transaction_suggestions WHERE id=?1",[id])?;
        if let Some(t) = s.transaction_id {
            return read_transaction(&tx, &t);
        }
        if s.status != SuggestionStatus::Pending
            || s.workspace_id != input.workspace_id
            || input.id.is_some()
        {
            return Err(AppError::invalid(
                "This suggestion is no longer pending or belongs to another workspace.",
            ));
        }
        if input.kind == TransactionKind::Transfer
            || account(&tx, &input.account_id)?.currency != s.currency
        {
            return Err(AppError::invalid(
                "Choose an income or expense account in the notification currency.",
            ));
        }
        let t = save_transaction_in(&tx, &input, TransactionSource::Notification)?;
        tx.execute("UPDATE transaction_suggestions SET status='accepted',transaction_id=?1,updated_at=?2 WHERE id=?3",params![t.id,now(),id])?;
        refresh_budget_alerts(&tx, &t.workspace_id, &t.occurred_on[..7])?;
        tx.commit()?;
        Ok(t)
    }
    pub fn dismiss_suggestion(&self, id: &str) -> Result<()> {
        if self.conn()?.execute("UPDATE transaction_suggestions SET status='dismissed',updated_at=?1 WHERE id=?2 AND status='pending'",params![now(),id])?==0{return Err(AppError::invalid("This suggestion is no longer pending."));}
        Ok(())
    }
    pub fn export_transactions_csv(&self, mut filter: TransactionFilter) -> Result<CsvExport> {
        let mut rows = Vec::new();
        filter.limit = 500;
        filter.offset = 0;
        loop {
            let page = self.transactions(filter.clone())?;
            rows.extend(page.items);
            if rows.len() >= page.total as usize {
                break;
            }
            filter.offset += 500;
            if rows.len() > 100000 {
                return Err(AppError::invalid("Choose a smaller export date range."));
            }
        }
        let mut csv=String::from("Date,Kind,Account,Currency,Amount,Receiving Account,Receiving Amount,Category,Payee,Note\r\n");
        fn cell(s: &str) -> String {
            let guarded = if s.starts_with(['=', '+', '-', '@', '\t', '\r']) {
                format!("'{s}")
            } else {
                s.into()
            };
            format!("\"{}\"", guarded.replace('"', "\"\""))
        }
        for t in &rows {
            let exp = currency_exponent(&t.currency)?;
            let factor = 10_i64.pow(exp);
            let amount = if exp == 0 {
                t.amount_minor.to_string()
            } else {
                format!(
                    "{}.{:0width$}",
                    t.amount_minor / factor,
                    t.amount_minor % factor,
                    width = exp as usize
                )
            };
            let fields = [
                t.occurred_on.clone(),
                key(t.kind),
                t.account_name.clone(),
                t.currency.clone(),
                amount,
                t.to_account_name.clone().unwrap_or_default(),
                t.to_amount_minor.map(|v| v.to_string()).unwrap_or_default(),
                t.category_name.clone().unwrap_or_default(),
                t.payee.clone().unwrap_or_default(),
                t.note.clone(),
            ];
            csv.push_str(&fields.iter().map(|f| cell(f)).collect::<Vec<_>>().join(","));
            csv.push_str("\r\n");
        }
        let eid = id();
        let file_name = format!("transactions-{eid}.csv");
        let rel = format!("exports/csv/{eid}.csv");
        let path = self.paths.resolve(&rel)?;
        std::fs::write(&path, csv)?;
        Ok(CsvExport {
            id: eid,
            file_name,
            path: path.to_string_lossy().into_owned(),
            rows: rows.len() as u32,
        })
    }
}
