use crate::{
    error::{AppError, Result},
    services::AppService,
    workspaces::Workspaces,
};
use std::sync::Arc;
use tauri::{Runtime, State};
mod business;
mod claims;
mod expenses;
mod exports;
mod jobs;
mod receipts;
mod settings;
pub(super) type Service<'a> = State<'a, Arc<Workspaces>>;
pub(super) async fn blocking<T: Send + 'static>(
    service: Service<'_>,
    f: impl FnOnce(Arc<AppService>) -> Result<T> + Send + 'static,
) -> Result<T> {
    // Resolved when the command arrives, so it runs against the account active then.
    let service = service.active();
    tauri::async_runtime::spawn_blocking(move || f(service))
        .await
        .map_err(|_| {
            AppError::new(
                "InternalError",
                "The background operation was interrupted. Saved receipts remain available.",
            )
        })?
}
/// The raw JSON arguments of a core command, forwarded unchanged to
/// `dispatch` on the blocking pool (D5 in docs/DECISIONS.md).
async fn forward(
    s: Service<'_>,
    command: &'static str,
    request: tauri::ipc::Request<'_>,
) -> Result<serde_json::Value> {
    let args = match request.body() {
        tauri::ipc::InvokeBody::Json(v) => v.clone(),
        tauri::ipc::InvokeBody::Raw(_) => serde_json::Value::Null,
    };
    let workspaces = s.inner().clone();
    tauri::async_runtime::spawn_blocking(move || workspaces.dispatch(command, args))
        .await
        .map_err(|_| {
            AppError::new(
                "InternalError",
                "The background operation was interrupted. Saved receipts remain available.",
            )
        })?
}
/// Starts Google or Apple sign-in in the system browser, never an embedded
/// view, so the provider's page is one the person can verify and trust.
#[tauri::command]
async fn cloud_oauth_start(
    s: Service<'_>,
    provider: String,
    link: Option<bool>,
) -> Result<serde_json::Value> {
    let workspaces = s.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        workspaces.sign_in_with_browser(&provider, link.unwrap_or(false), &|url| {
            open::that(url)
                .map_err(|_| AppError::new("OpenError", "Unable to open the browser to sign in."))
        })
    })
    .await
    .map_err(|_| AppError::new("InternalError", "Sign-in was interrupted."))?
}
/// Builds the handler from the hand-written commands plus one generated Tauri
/// command per core command name. A new core operation needs its `dispatch.rs`
/// entry and its name in the second list; nothing else.
macro_rules! handler_with_core_commands {
    ([$($legacy:path),* $(,)?], [$($core:ident),* $(,)?]) => {
        $(
            #[tauri::command]
            async fn $core(
                s: Service<'_>,
                request: tauri::ipc::Request<'_>,
            ) -> Result<serde_json::Value> {
                forward(s, stringify!($core), request).await
            }
        )*
        pub fn handler<R: Runtime>(
        ) -> impl Fn(tauri::ipc::Invoke<R>) -> bool + Send + Sync + 'static {
            tauri::generate_handler![$($legacy,)* $($core),*]
        }
    };
}
handler_with_core_commands!(
    [
        cloud_oauth_start,
        exports::open_data_export,
        business::open_invoice_pdf,
        expenses::list_expenses,
        expenses::get_expense,
        expenses::create_expense,
        expenses::edit_expense,
        expenses::queue_extraction,
        expenses::delete_expenses,
        expenses::convert_pending,
        receipts::import_receipts,
        receipts::read_receipt,
        receipts::read_thumbnail,
        claims::list_claims,
        claims::get_claim,
        claims::create_claim,
        claims::edit_claim,
        claims::set_claim_expense,
        claims::add_claim_expenses,
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
        jobs::online_available,
        jobs::try_online,
        jobs::complete_ocr,
        jobs::complete_thumbnail,
        jobs::export_snapshot,
        jobs::complete_pdf
    ],
    [
        workspace_info,
        cloud_status,
        cloud_sync,
        cloud_sign_in,
        cloud_sign_out,
        cloud_request_code,
        cloud_providers,
        cloud_verify_code,
        cloud_oauth_finish,
        cloud_oauth_cancel,
        cloud_adopt,
        cloud_diagnose,
        cloud_conflicts,
        cloud_resolve_conflict,
        // Ledger (services::finance)
        list_workspaces,
        save_workspace,
        list_categories,
        save_category,
        delete_category,
        list_accounts,
        save_account,
        delete_account,
        list_transactions,
        get_transaction,
        save_transaction,
        delete_transactions,
        record_expense,
        list_budgets,
        save_budget,
        delete_budget,
        list_bills,
        get_bill,
        save_bill,
        delete_bill,
        list_bill_occurrences,
        pay_bill,
        report_monthly,
        report_overview,
        export_transactions_csv,
        export_all_data,
        list_reminders,
        ack_reminder,
        parse_quick_add,
        list_suggestions,
        accept_suggestion,
        dismiss_suggestion,
        get_receipt_items,
        save_receipt_items,
        save_invoice_pdf,
        ingest_payment_notification,
        get_business_profile,
        save_business_profile,
        list_customers,
        save_customer,
        list_inventory_items,
        save_inventory_item,
        delete_customer,
        delete_inventory_item,
        adjust_stock,
        list_stock_movements,
        list_invoices,
        get_invoice,
        save_invoice,
        issue_invoice,
        void_invoice,
        record_invoice_payment,
        report_business,
        get_tax_rules,
        estimate_income_tax,
        calculate_sst,
        validate_einvoice,
    ]
);
