//! Restrictive local notification parsing. Only supported financial apps and
//! explicit completed debit messages can become suggestions; never transactions.
use super::AppService;
use crate::{
    domain::{expense::parse_money, finance::TransactionSuggestion, id, now},
    error::{AppError, Result},
};
use rusqlite::{params, OptionalExtension};
use serde::Deserialize;
use sha2::{Digest, Sha256};
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PaymentNotificationInput {
    pub package_name: String,
    pub notification_key: String,
    pub posted_at: i64,
    pub text: String,
    pub occurred_on: String,
}
impl AppService {
    pub fn ingest_payment_notification(
        &self,
        input: PaymentNotificationInput,
    ) -> Result<Option<TransactionSuggestion>> {
        let label = match input.package_name.as_str() {
            "my.com.maybank2u.m2umobile" => "Maybank",
            "my.com.tngdigital.ewallet" => "Touch 'n Go eWallet",
            _ => return Ok(None),
        };
        if input.text.len() > 4000 || input.notification_key.len() > 500 || input.posted_at < 0 {
            return Ok(None);
        }
        super::finance::date(&input.occurred_on)?;
        let lower = input.text.to_lowercase();
        // A balance, OTP, request, failed or reversed payment is not spending.
        if [
            "otp",
            "tac",
            "verification",
            "unsuccessful",
            "failed",
            "declined",
            "reversed",
            "refund",
            "request",
            "received",
            "credited",
            "scheduled",
        ]
        .iter()
        .any(|s| lower.contains(s))
        {
            return Ok(None);
        }
        let complete = [
            "payment successful",
            "payment of",
            "you paid",
            "you spent",
            "debited",
            "successfully paid",
        ]
        .iter()
        .any(|s| lower.contains(s));
        if !complete {
            return Ok(None);
        }
        let re = regex::Regex::new(r"(?i)\b(?:RM|MYR)\s*([0-9]+(?:,[0-9]{3})*(?:\.[0-9]{1,2})?)\b")
            .unwrap();
        let amounts: Vec<_> = re.captures_iter(&input.text).collect();
        // Do not guess when a message also contains a balance or second amount.
        if amounts.len() != 1 {
            return Ok(None);
        }
        let amount = parse_money(&amounts[0][1], 2)?;
        if amount <= 0 {
            return Ok(None);
        }
        let mut db = self.conn()?;
        let tx = db.transaction()?;
        let settings = Self::settings_in(&tx)?;
        if !settings.payment_alerts_enabled {
            return Ok(None);
        }
        let w: String =
            tx.query_row("SELECT id FROM workspaces WHERE kind='personal'", [], |r| {
                r.get(0)
            })?;
        let fingerprint = format!(
            "{:x}",
            Sha256::digest(
                format!(
                    "{}\0{}\0{}\0{}\0{}",
                    input.package_name,
                    input.notification_key,
                    input.posted_at,
                    amount,
                    input.occurred_on
                )
                .as_bytes()
            )
        );
        let existing: Option<String> = tx
            .query_row(
                "SELECT id FROM transaction_suggestions WHERE fingerprint=?1",
                [&fingerprint],
                |r| r.get(0),
            )
            .optional()?;
        let sid = existing.unwrap_or_else(id);
        tx.execute("INSERT OR IGNORE INTO transaction_suggestions(id,workspace_id,source,fingerprint,kind,amount_minor,currency,occurred_on,payee,app_label,created_at,updated_at) VALUES(?1,?2,'notification',?3,'expense',?4,'MYR',?5,NULL,?6,?7,?7)",params![sid,w,fingerprint,amount,input.occurred_on,label,now()])?;
        let out=super::finance::query::<TransactionSuggestion>(&tx,"SELECT id,workspace_id,kind,amount_minor,currency,occurred_on,payee,app_label,status,transaction_id,created_at FROM transaction_suggestions WHERE id=?1",[&sid])?.into_iter().next().ok_or_else(AppError::not_found)?;
        tx.commit()?;
        Ok(Some(out))
    }
}
