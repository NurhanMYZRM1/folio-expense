use super::{blocking, Service};
use crate::domain::{
    claim::ExportSnapshot,
    extraction::{Job, JobLease},
};
use crate::error::Result;

#[tauri::command]
pub(super) async fn list_jobs(s: Service<'_>) -> Result<Vec<Job>> {
    blocking(s, |s| s.jobs()).await
}
#[tauri::command]
pub(super) async fn take_job(s: Service<'_>) -> Result<Option<JobLease>> {
    blocking(s, |s| s.take_job()).await
}
#[tauri::command]
pub(super) async fn heartbeat(s: Service<'_>, id: String, token: String) -> Result<()> {
    blocking(s, move |s| s.heartbeat(&id, &token)).await
}
#[tauri::command]
pub(super) async fn fail_job(
    s: Service<'_>,
    id: String,
    token: String,
    message: String,
) -> Result<()> {
    blocking(s, move |s| s.fail_job(&id, &token, &message)).await
}
#[tauri::command]
pub(super) async fn retry_job(s: Service<'_>, id: String) -> Result<()> {
    blocking(s, move |s| s.retry_job(&id)).await
}
#[tauri::command]
pub(super) async fn try_online(
    s: Service<'_>,
    id: String,
    token: String,
    images: Vec<String>,
) -> Result<bool> {
    blocking(s, move |s| s.try_online(&id, &token, images)).await
}
#[tauri::command]
pub(super) async fn complete_ocr(
    s: Service<'_>,
    id: String,
    token: String,
    raw_text: String,
    ocr_confidence: Option<f64>,
) -> Result<()> {
    blocking(s, move |s| {
        s.complete_ocr(&id, &token, raw_text, ocr_confidence)
    })
    .await
}
#[tauri::command]
pub(super) async fn complete_thumbnail(
    s: Service<'_>,
    id: String,
    token: String,
    base64: String,
) -> Result<()> {
    blocking(s, move |s| s.complete_thumbnail(&id, &token, &base64)).await
}
#[tauri::command]
pub(super) async fn export_snapshot(
    s: Service<'_>,
    id: String,
    token: String,
) -> Result<ExportSnapshot> {
    blocking(s, move |s| s.export_snapshot(&id, &token)).await
}
#[tauri::command]
pub(super) async fn complete_pdf(
    s: Service<'_>,
    id: String,
    token: String,
    base64: String,
) -> Result<String> {
    blocking(s, move |s| s.complete_pdf(&id, &token, &base64)).await
}
