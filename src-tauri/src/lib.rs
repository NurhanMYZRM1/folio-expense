#[cfg(feature = "desktop")]
mod commands;
pub mod db;
pub mod domain;
pub mod error;
pub mod jobs;
pub mod repository;
pub mod security;
pub mod services;
pub mod storage;
#[cfg(feature = "desktop")]
pub fn run() {
    use tauri::Manager;
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _, _| {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.set_focus();
            }
        }))
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_deep_link::init())
        .setup(|app| {
            let root = app.path().app_data_dir()?.join("expense-app");
            let service = services::AppService::open(
                root,
                std::sync::Arc::new(security::secrets::OsSecretStore),
            )?;
            app.manage(std::sync::Arc::new(service));
            Ok(())
        })
        .invoke_handler(commands::handler())
        .run(tauri::generate_context!())
        .expect("Folio could not start. Check that the application data directory is writable.");
}
