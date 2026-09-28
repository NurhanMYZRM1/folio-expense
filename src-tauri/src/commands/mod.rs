use crate::{
    error::{AppError, Result},
    services::AppService,
};
use std::sync::Arc;
use tauri::{Runtime, State};
mod claims;
mod expenses;
mod exports;
mod jobs;
mod receipts;
mod settings;
pub(super) type Service<'a> = State<'a, Arc<AppService>>;
pub(super) async fn blocking<T: Send + 'static>(
    service: Service<'_>,
    f: impl FnOnce(Arc<AppService>) -> Result<T> + Send + 'static,
) -> Result<T> {
    let service = service.inner().clone();
    tauri::async_runtime::spawn_blocking(move || f(service))
        .await
        .map_err(|_| {
            AppError::new(
                "InternalError",
                "The background operation was interrupted. Saved receipts remain available.",
            )
        })?
}
pub fn handler<R: Runtime>() -> impl Fn(tauri::ipc::Invoke<R>) -> bool + Send + Sync + 'static {
    tauri::generate_handler![
        expenses::list_expenses,
        expenses::get_expense,
        expenses::create_expense,
        expenses::edit_expense,
        expenses::queue_extraction,
        receipts::import_receipts,
        receipts::read_receipt,
        receipts::read_thumbnail,
        claims::list_claims,
        claims::get_claim,
        claims::create_claim,
        claims::edit_claim,
        claims::set_claim_expense,
        claims::transition_claim,
        claims::request_pdf,
        claims::open_export,
        exports::export_claim_csv,
        exports::export_expenses_csv,
        exports::export_claim_xlsx,
        exports::export_expenses_xlsx,
        exports::open_csv_export,
        exports::open_xlsx_export,
        settings::get_settings,
        settings::save_settings,
        settings::app_info,
        settings::set_credential,
        settings::delete_credential,
        jobs::list_jobs,
        jobs::take_job,
        jobs::heartbeat,
        jobs::fail_job,
        jobs::retry_job,
        jobs::try_online,
        jobs::complete_ocr,
        jobs::complete_thumbnail,
        jobs::export_snapshot,
        jobs::complete_pdf
    ]
}
