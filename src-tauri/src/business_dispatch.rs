//! Business/tax commands share this dispatcher on desktop, mobile and tests.
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
    let workspace = || a["workspaceId"].as_str().unwrap_or("");
    let id = || a["id"].as_str().unwrap_or("");
    match command {
        "save_invoice_pdf" => out!(s.save_invoice_pdf(
            workspace(),
            a["invoiceId"].as_str().unwrap_or(""),
            arg!("version"),
            a["base64"].as_str().unwrap_or("")
        )),
        "open_invoice_pdf" => out!(s.invoice_pdf_path(id())),
        "ingest_payment_notification" => out!(s.ingest_payment_notification(arg!("input"))),
        "get_receipt_items" => out!(s.get_receipt_items(a["expenseId"].as_str().unwrap_or(""))),
        "save_receipt_items" => out!(s.save_receipt_items(arg!("input"))),
        "get_business_profile" => out!(s.get_business_profile(workspace())),
        "save_business_profile" => out!(s.save_business_profile(arg!("input"))),
        "list_customers" => {
            out!(s.list_customers(workspace(), a["includeArchived"].as_bool().unwrap_or(false)))
        }
        "save_customer" => out!(s.save_customer(arg!("input"))),
        "list_inventory_items" => {
            out!(s
                .list_inventory_items(workspace(), a["includeArchived"].as_bool().unwrap_or(false)))
        }
        "save_inventory_item" => out!(s.save_inventory_item(arg!("input"))),
        "adjust_stock" => out!(s.adjust_stock(arg!("input"))),
        "list_stock_movements" => {
            out!(s.list_stock_movements(workspace(), a["itemId"].as_str().unwrap_or("")))
        }
        "list_invoices" => out!(s.list_invoices(workspace())),
        "get_invoice" => out!(s.get_invoice(workspace(), id())),
        "save_invoice" => out!(s.save_invoice(arg!("input"))),
        "issue_invoice" => out!(s.issue_invoice(workspace(), id(), arg!("version"))),
        "void_invoice" => out!(s.void_invoice(workspace(), id(), arg!("version"))),
        "record_invoice_payment" => out!(s.record_invoice_payment(arg!("input"))),
        "report_business" => {
            out!(s.report_business(workspace(), a["today"].as_str().unwrap_or("")))
        }
        "get_tax_rules" => out!(s.get_tax_rules(arg!("assessmentYear"))),
        "estimate_income_tax" => out!(s.estimate_income_tax(arg!("input"))),
        "calculate_sst" => out!(s.calculate_sst(arg!("input"))),
        "validate_einvoice" => {
            out!(s.validate_einvoice(workspace(), a["invoiceId"].as_str().unwrap_or("")))
        }
        _ => Err(AppError::invalid("Unknown command.")),
    }
}
