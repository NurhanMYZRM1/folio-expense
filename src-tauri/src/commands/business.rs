use super::{blocking, Service};
use crate::error::{AppError, Result};
#[tauri::command]
pub async fn open_invoice_pdf(s: Service<'_>, id: String) -> Result<()> {
    blocking(s, move |s| {
        let path = s.invoice_pdf_path(&id)?;
        open::that(path).map_err(|_| AppError::invalid("The invoice PDF could not be opened."))
    })
    .await
}
