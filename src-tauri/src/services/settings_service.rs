use super::AppService;
use crate::{
    domain::{
        expense::currency_exponent,
        now,
        settings::{AppInfo, Settings},
    },
    error::{AppError, Result},
    repository,
};
use rusqlite::{params, OptionalExtension};
use zeroize::Zeroizing;
impl AppService {
    pub fn settings(&self) -> Result<Settings> {
        let db = self.conn()?;
        Self::settings_in(&db)
    }
    pub(crate) fn settings_in(db: &rusqlite::Connection) -> Result<Settings> {
        let value: Option<String> = db
            .query_row(
                "SELECT value FROM settings WHERE key='preferences'",
                [],
                |r| r.get(0),
            )
            .optional()?;
        value
            .map(|v| serde_json::from_str(&v).map_err(Into::into))
            .unwrap_or_else(|| Ok(Settings::default()))
    }
    pub fn save_settings(&self, s: Settings) -> Result<Settings> {
        if !["light", "dark", "system"].contains(&s.theme.as_str())
            || s.provider != "openai_compatible"
        {
            return Err(AppError::invalid("Invalid setting."));
        }
        currency_exponent(&s.default_currency)?;
        let url = reqwest::Url::parse(&s.api_base_url)
            .map_err(|_| AppError::invalid("Enter a valid HTTPS provider URL."))?;
        if url.scheme() != "https"
            || url.host_str().is_none()
            || !url.username().is_empty()
            || url.password().is_some()
            || url.query().is_some()
            || url.fragment().is_some()
        {
            return Err(AppError::invalid("The AI provider must use HTTPS without embedded credentials, a query or a fragment."));
        }
        if s.ai_model.trim().is_empty()
            || s.ai_model.len() > 100
            || s.company.len() > 200
            || s.employee.len() > 200
        {
            return Err(AppError::invalid(
                "Check the model, company and employee fields.",
            ));
        }
        if let Some(ref p) = s.export_directory {
            if !std::path::Path::new(p).is_absolute() || !std::path::Path::new(p).is_dir() {
                return Err(AppError::invalid("Choose an existing export directory."));
            }
        }
        let mut db = self.conn()?;
        let tx = db.transaction()?;
        let old = Self::settings_in(&tx)?;
        if old.api_base_url.trim_end_matches('/') != s.api_base_url.trim_end_matches('/') {
            self.secrets.delete()?;
        }
        tx.execute("INSERT INTO settings(key,value,updated_at) VALUES('preferences',?1,?2) ON CONFLICT(key) DO UPDATE SET value=excluded.value,updated_at=excluded.updated_at,version=settings.version+1,sync_state='local_only'",params![serde_json::to_string(&s)?,now()])?;
        repository::audit(
            &tx,
            "settings.edited",
            "preferences",
            serde_json::json!({"onlineEnabled":s.online_enabled}),
        )?;
        tx.commit()?;
        // A changed URL, model or key deserves a fresh try after a provider failure.
        self.online_backoff().clear();
        Ok(s)
    }
    pub fn set_credential(&self, key: String) -> Result<()> {
        let key = Zeroizing::new(key);
        if key.trim().is_empty() || key.len() > 4096 {
            return Err(AppError::invalid("Enter a valid API credential."));
        }
        self.secrets.set(key.trim())?;
        self.online_backoff().clear();
        Ok(())
    }
    pub fn delete_credential(&self) -> Result<()> {
        self.secrets.delete()?;
        self.online_backoff().clear();
        Ok(())
    }
    /// Permanently erases every local record, receipt, generated report and the
    /// stored AI credential, leaving an empty workspace with default settings.
    /// Copies the user saved elsewhere (an export folder, a share sheet) are theirs and untouched.
    pub fn erase_all_data(&self) -> Result<()> {
        let mut db = self.conn()?;
        let cutoff = (chrono::Utc::now() - chrono::Duration::minutes(2))
            .to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
        let busy: bool = db.query_row(
            "SELECT EXISTS(SELECT 1 FROM jobs WHERE status='running' AND updated_at >= ?1)",
            [cutoff],
            |r| r.get(0),
        )?;
        if busy {
            return Err(AppError::invalid(
                "Folio is still reading a receipt or building a report. Try again when it finishes.",
            ));
        }
        // The credential goes first: if the vault refuses, nothing has been erased yet.
        self.secrets.delete()?;
        let tx = db.transaction()?;
        tx.execute_batch(
            "DELETE FROM claim_expenses; DELETE FROM extraction_runs; DELETE FROM jobs;
             DELETE FROM expenses; DELETE FROM receipt_files; DELETE FROM expense_claims;
             DELETE FROM audit_events; DELETE FROM exchange_rates; DELETE FROM settings;",
        )?;
        tx.commit()?;
        // Rewrite the file so erased rows do not linger in free pages or the WAL.
        db.execute_batch("VACUUM; PRAGMA wal_checkpoint(TRUNCATE);")?;
        drop(db);
        self.online_backoff().clear();
        // Receipt folders are now unreferenced, so the startup cleanup removes them.
        self.recover_storage()?;
        for folder in ["exports/claims", "exports/csv", "exports/xlsx", "cache"] {
            for entry in std::fs::read_dir(self.paths.resolve(folder)?)? {
                let entry = entry?;
                if entry.file_type()?.is_file() {
                    std::fs::remove_file(entry.path()).map_err(|_| {
                        AppError::new(
                            "StorageError",
                            "Your records were erased, but a saved report could not be removed. Close any open Folio report and choose Erase all data again.",
                        )
                    })?;
                }
            }
        }
        Ok(())
    }
    pub fn app_info(&self) -> Result<AppInfo> {
        Ok(AppInfo {
            data_directory: self.paths.root.to_string_lossy().to_string(),
            version: env!("CARGO_PKG_VERSION").into(),
            credential_configured: self.secrets.get()?.is_some(),
        })
    }
}
