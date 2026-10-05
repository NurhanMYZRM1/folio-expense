use super::{blocking, Service};
use crate::domain::claim::{Claim, ClaimDetail, ClaimStatus};
use crate::error::AppError;
use crate::error::Result;

#[tauri::command]
pub(super) async fn list_claims(s: Service<'_>) -> Result<Vec<Claim>> {
    blocking(s, |s| s.claims()).await
}
#[tauri::command]
pub(super) async fn get_claim(s: Service<'_>, id: String) -> Result<ClaimDetail> {
    blocking(s, move |s| s.claim(&id)).await
}
#[tauri::command]
pub(super) async fn create_claim(s: Service<'_>, title: String, currency: String) -> Result<Claim> {
    blocking(s, move |s| s.create_claim(title, currency)).await
}
#[tauri::command]
pub(super) async fn edit_claim(
    s: Service<'_>,
    id: String,
    version: i32,
    title: String,
    description: String,
) -> Result<Claim> {
    blocking(s, move |s| s.edit_claim(&id, version, title, description)).await
}
#[tauri::command]
pub(super) async fn set_claim_expense(
    s: Service<'_>,
    claim_id: String,
    expense_id: String,
    add: bool,
) -> Result<ClaimDetail> {
    blocking(s, move |s| s.set_claim_expense(&claim_id, &expense_id, add)).await
}
#[tauri::command]
pub(super) async fn add_claim_expenses(
    s: Service<'_>,
    claim_id: String,
    expense_ids: Vec<String>,
) -> Result<ClaimDetail> {
    blocking(s, move |s| s.add_claim_expenses(&claim_id, expense_ids)).await
}
#[tauri::command]
pub(super) async fn transition_claim(
    s: Service<'_>,
    id: String,
    status: ClaimStatus,
) -> Result<Claim> {
    blocking(s, move |s| s.transition_claim(&id, status)).await
}
#[tauri::command]
pub(super) async fn request_pdf(s: Service<'_>, id: String) -> Result<String> {
    blocking(s, move |s| s.request_pdf(&id)).await
}
#[tauri::command]
pub(super) async fn open_export(s: Service<'_>, id: String) -> Result<()> {
    blocking(s, move |s| {
        let path = s.export_path(&id)?;
        open::that(path).map_err(|_| {
            AppError::new("OpenError", "Unable to open the PDF in the system viewer.")
        })?;
        Ok(())
    })
    .await
}
