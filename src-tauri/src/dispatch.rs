//! One JSON entry point over every AppService operation, mirroring the Tauri
//! commands. Used by the test driver (stdio) and the mobile FFI.
use crate::{
    error::{AppError, Result},
    services::AppService,
};
use serde_json::Value;

pub fn dispatch(s: &AppService, command: &str, a: Value) -> Result<Value> {
    macro_rules! arg {
        ($key:literal) => {
            serde_json::from_value(
                a.get($key)
                    .cloned()
                    .ok_or_else(|| AppError::invalid(concat!("Missing ", $key)))?,
            )?
        };
    }
    macro_rules! out {
        ($e:expr) => {
            Ok(serde_json::to_value($e?)?)
        };
    }
    let id = || a["id"].as_str().unwrap_or("");
    let token = || a["token"].as_str().unwrap_or("");
    match command {
        "list_expenses" => out!(s.expenses()),
        "get_expense" => out!(s.expense(id())),
        "create_expense" => out!(s.create_expense()),
        "edit_expense" => out!(s.edit_expense(arg!("edit"))),
        "queue_extraction" => out!(s.queue_extraction(id())),
        "delete_expenses" => out!(s.delete_expenses(arg!("ids"))),
        "convert_pending" => out!(s.convert_pending()),
        "add_claim_expenses" => out!(s.add_claim_expenses(
            a["claimId"]
                .as_str()
                .ok_or_else(|| AppError::invalid("Expected claimId"))?,
            arg!("expenseIds")
        )),
        "import_receipts" => out!(s.import_receipts(arg!("paths"))),
        "read_receipt" => out!(s.read_receipt(id())),
        "read_thumbnail" => out!(s.read_thumbnail(id())),
        "list_claims" => out!(s.claims()),
        "get_claim" => out!(s.claim(id())),
        "create_claim" => out!(s.create_claim(arg!("title"), arg!("currency"))),
        "edit_claim" => {
            out!(s.edit_claim(id(), arg!("version"), arg!("title"), arg!("description")))
        }
        "set_claim_expense" => out!(s.set_claim_expense(
            a["claimId"]
                .as_str()
                .ok_or_else(|| AppError::invalid("Expected string"))?,
            a["expenseId"]
                .as_str()
                .ok_or_else(|| AppError::invalid("Expected string"))?,
            arg!("add")
        )),
        "transition_claim" => out!(s.transition_claim(id(), arg!("status"))),
        "request_pdf" => out!(s.request_pdf(id())),
        "get_settings" => out!(s.settings()),
        "save_settings" => out!(s.save_settings(arg!("settings"))),
        "app_info" => out!(s.app_info()),
        "list_jobs" => out!(s.jobs()),
        "take_job" => out!(s.take_job()),
        "heartbeat" => out!(s.heartbeat(id(), token())),
        "fail_job" => out!(s.fail_job(
            id(),
            token(),
            a["message"]
                .as_str()
                .ok_or_else(|| AppError::invalid("Expected string"))?
        )),
        "retry_job" => out!(s.retry_job(id())),
        "online_available" => out!(s.online_available()),
        "try_online" => out!(s.try_online(id(), token(), arg!("images"))),
        "complete_ocr" => out!(s.complete_ocr(
            id(),
            token(),
            arg!("rawText"),
            a.get("ocrConfidence").and_then(serde_json::Value::as_f64)
        )),
        "complete_thumbnail" => out!(s.complete_thumbnail(
            id(),
            token(),
            a["base64"]
                .as_str()
                .ok_or_else(|| AppError::invalid("Expected string"))?
        )),
        "export_snapshot" => out!(s.export_snapshot(id(), token())),
        "complete_pdf" => out!(s.complete_pdf(
            id(),
            token(),
            a["base64"]
                .as_str()
                .ok_or_else(|| AppError::invalid("Expected string"))?
        )),
        "open_export" => out!(s.export_path(id())),
        "export_claim_csv" => out!(s.export_claim_csv(id())),
        "export_expenses_csv" => out!(s.export_expenses_csv(arg!("ids"))),
        // Mirrors the "open_export" test shim: never launches the system app in tests,
        // just resolves the file path so tests can read the CSV directly.
        "open_csv_export" => out!(s.csv_export_path(id())),
        "export_claim_xlsx" => out!(s.export_claim_xlsx(id())),
        "export_expenses_xlsx" => out!(s.export_expenses_xlsx(arg!("ids"))),
        "open_xlsx_export" => out!(s.xlsx_export_path(id())),
        _ => Err(AppError::invalid("Unknown command.")),
    }
}
