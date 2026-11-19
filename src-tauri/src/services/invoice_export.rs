//! User-initiated invoice PDFs, rendered from a versioned core snapshot.
use super::{csv_export::CsvExport, AppService};
use crate::{
    domain::id,
    error::{AppError, Result},
};
use base64::Engine;
use std::io::Write;
impl AppService {
    pub fn save_invoice_pdf(
        &self,
        workspace: &str,
        invoice: &str,
        version: i32,
        encoded: &str,
    ) -> Result<CsvExport> {
        let snapshot = self.get_invoice(workspace, invoice)?;
        if snapshot.version != version {
            return Err(AppError::new(
                "Conflict",
                "This invoice changed. Reload before exporting.",
            ));
        }
        if encoded.len() > 14_000_000 {
            return Err(AppError::invalid("The invoice PDF is too large."));
        }
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(encoded)
            .map_err(|_| AppError::invalid("Invalid invoice PDF."))?;
        if !bytes.starts_with(b"%PDF-") {
            return Err(AppError::invalid("Invalid invoice PDF."));
        }
        let export_id = id();
        let relative = format!("exports/invoices/{export_id}.pdf");
        let path = self.paths.resolve(&relative)?;
        std::fs::create_dir_all(path.parent().unwrap())?;
        let mut output = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)?;
        output.write_all(&bytes)?;
        output.sync_all()?;
        Ok(CsvExport {
            id: export_id,
            file_name: format!(
                "{}.pdf",
                snapshot.number.as_deref().unwrap_or("Draft invoice")
            ),
            path: path.to_string_lossy().into_owned(),
            rows: snapshot.lines.len() as u32,
        })
    }
    pub fn invoice_pdf_path(&self, id: &str) -> Result<String> {
        uuid::Uuid::parse_str(id).map_err(|_| AppError::invalid("Invalid export ID."))?;
        let path = self.paths.resolve(&format!("exports/invoices/{id}.pdf"))?;
        if !path.is_file() {
            return Err(AppError::not_found());
        }
        Ok(path.to_string_lossy().into_owned())
    }
}
