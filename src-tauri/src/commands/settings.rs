use super::{blocking, Service};
use crate::domain::settings::{AppInfo, Settings};
use crate::error::Result;

#[tauri::command]
pub(super) async fn get_settings(s: Service<'_>) -> Result<Settings> {
    blocking(s, |s| s.settings()).await
}
#[tauri::command]
pub(super) async fn save_settings(s: Service<'_>, settings: Settings) -> Result<Settings> {
    blocking(s, move |s| s.save_settings(settings)).await
}
#[tauri::command]
pub(super) async fn app_info(s: Service<'_>) -> Result<AppInfo> {
    blocking(s, |s| s.app_info()).await
}
#[tauri::command]
pub(super) async fn set_credential(s: Service<'_>, key: String) -> Result<()> {
    blocking(s, move |s| s.set_credential(key)).await
}
#[tauri::command]
pub(super) async fn delete_credential(s: Service<'_>) -> Result<()> {
    blocking(s, |s| s.delete_credential()).await
}
