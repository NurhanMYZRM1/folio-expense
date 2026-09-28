use super::{blocking, Service};
use crate::error::{AppError, Result};
use crate::services::csv_export::CsvExport;
#[tauri::command]
pub(super) async fn export_claim_csv(s: Service<'_>, id: String) -> Result<CsvExport> {
    blocking(s, move |s| s.export_claim_csv(&id)).await
}
#[tauri::command]
pub(super) async fn export_expenses_csv(s: Service<'_>, ids: Vec<String>) -> Result<CsvExport> {
    blocking(s, move |s| s.export_expenses_csv(ids)).await
}
#[tauri::command]
pub(super) async fn export_claim_xlsx(s: Service<'_>, id: String) -> Result<CsvExport> {
    blocking(s, move |s| s.export_claim_xlsx(&id)).await
}
#[tauri::command]
pub(super) async fn export_expenses_xlsx(s: Service<'_>, ids: Vec<String>) -> Result<CsvExport> {
    blocking(s, move |s| s.export_expenses_xlsx(ids)).await
}
#[tauri::command]
pub(super) async fn open_csv_export(s: Service<'_>, id: String) -> Result<()> {
    blocking(s, move |s| {
        let path = s.csv_export_path(&id)?;
        open::that(path).map_err(|_| {
            AppError::new(
                "OpenError",
                "Unable to open the CSV in the system application.",
            )
        })?;
        Ok(())
    })
    .await
}
#[tauri::command]
pub(super) async fn open_xlsx_export(s: Service<'_>, id: String) -> Result<()> {
    blocking(s, move |s| {
        let path = s.xlsx_export_path(&id)?;
        open::that(path).map_err(|_| {
            AppError::new(
                "OpenError",
                "Unable to open the Excel workbook in the system application.",
            )
        })?;
        Ok(())
    })
    .await
}
