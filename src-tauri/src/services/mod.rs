pub mod claim_service;
pub mod csv_export;
pub mod exchange_rates;
pub mod expense_service;
pub mod extraction;
pub mod receipt_service;
pub mod settings_service;
use crate::{
    db,
    error::{AppError, Result},
    security::secrets::SecretStore,
    storage::paths::AppPaths,
};
use rusqlite::Connection;
use std::{
    path::PathBuf,
    sync::{Arc, Mutex, MutexGuard},
};
pub struct AppService {
    pub(crate) db: Mutex<Connection>,
    pub paths: AppPaths,
    pub(crate) secrets: Arc<dyn SecretStore>,
    pub(crate) rates: Arc<dyn exchange_rates::RateSource>,
}
impl AppService {
    pub fn open(root: PathBuf, secrets: Arc<dyn SecretStore>) -> Result<Self> {
        let paths = AppPaths::new(root)?;
        let db = db::open(&paths.resolve("database/expenses.sqlite")?)?;
        let service = Self {
            db: Mutex::new(db),
            paths,
            secrets,
            rates: Arc::new(exchange_rates::FrankfurterRates),
        };
        service.recover_jobs()?;
        service.recover_storage()?;
        Ok(service)
    }
    /// Replaces where exchange rates come from (tests use a fixed table).
    pub fn with_rate_source(mut self, rates: Arc<dyn exchange_rates::RateSource>) -> Self {
        self.rates = rates;
        self
    }
    pub(crate) fn conn(&self) -> Result<MutexGuard<'_, Connection>> {
        self.db.lock().map_err(|_| {
            AppError::new(
                "DatabaseError",
                "The database is busy. Restart Folio to recover pending work.",
            )
        })
    }
}
