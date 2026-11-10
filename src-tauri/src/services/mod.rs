pub mod business;
pub mod claim_service;
pub mod cloud_sync;
pub mod cloud_transport;
pub mod csv_export;
pub mod data_export;
pub mod exchange_rates;
pub mod expense_service;
pub mod extraction;
pub mod finance;
pub mod invoice_export;
pub mod payment_notifications;
pub mod receipt_items;
pub mod receipt_service;
pub mod settings_service;
pub mod tax;
use crate::{
    db,
    error::{AppError, Result},
    security::secrets::SecretStore,
    storage::paths::AppPaths,
};
use rusqlite::Connection;
use std::{
    path::PathBuf,
    sync::{atomic::AtomicU64, Arc, Mutex, MutexGuard},
};
pub struct AppService {
    pub(crate) db: Mutex<Connection>,
    /// The one sync worker and session-refresh owner for this workspace.
    pub(crate) cloud_lock: Mutex<Option<cloud_sync::Access>>,
    pub(crate) cloud: Arc<cloud_transport::Transport>,
    /// Bumped on sign-out and account change; a sync started earlier stops writing.
    pub(crate) cloud_generation: AtomicU64,
    pub paths: AppPaths,
    pub(crate) secrets: Arc<dyn SecretStore>,
    pub(crate) rates: Arc<dyn exchange_rates::RateSource>,
    /// Skips the AI provider for a while after it fails (see `extraction::backoff`).
    pub(crate) online_backoff: Mutex<extraction::backoff::OnlineBackoff>,
}
impl AppService {
    pub fn open(root: PathBuf, secrets: Arc<dyn SecretStore>) -> Result<Self> {
        let cloud = cloud_transport::Transport::new(cloud_transport::CloudConfig::current()?)?;
        Self::open_with_cloud(root, secrets, Arc::new(cloud))
    }
    /// Opens a workspace that talks to the given cloud project (shared by every
    /// account workspace of one app, and replaced by a local server in tests).
    pub fn open_with_cloud(
        root: PathBuf,
        secrets: Arc<dyn SecretStore>,
        cloud: Arc<cloud_transport::Transport>,
    ) -> Result<Self> {
        let paths = AppPaths::new(root)?;
        let db = db::open(&paths.resolve("database/expenses.sqlite")?)?;
        let service = Self {
            db: Mutex::new(db),
            cloud_lock: Mutex::new(None),
            cloud,
            cloud_generation: AtomicU64::new(0),
            paths,
            secrets,
            rates: Arc::new(exchange_rates::FrankfurterRates),
            online_backoff: Mutex::default(),
        };
        service.initialize_finance()?;
        service.initialize_cloud()?;
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
