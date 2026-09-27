use super::{blocking, Service};
use crate::domain::receipt::{ImportOutcome, ReceiptContent};
use crate::error::Result;

#[tauri::command]
pub(super) async fn import_receipts(
    s: Service<'_>,
    paths: Vec<String>,
) -> Result<Vec<ImportOutcome>> {
    blocking(s, move |s| s.import_receipts(paths)).await
}
#[tauri::command]
pub(super) async fn read_receipt(s: Service<'_>, id: String) -> Result<ReceiptContent> {
    blocking(s, move |s| s.read_receipt(&id)).await
}
#[tauri::command]
pub(super) async fn read_thumbnail(s: Service<'_>, id: String) -> Result<Option<String>> {
    blocking(s, move |s| s.read_thumbnail(&id)).await
}
