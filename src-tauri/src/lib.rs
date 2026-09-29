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
            match services::AppService::open(
                root.clone(),
                std::sync::Arc::new(security::secrets::OsSecretStore),
            ) {
                Ok(service) => {
                    app.manage(std::sync::Arc::new(service));
                }
                // Never return this error to Tauri: on macOS it turns a setup
                // error into an abort ("non-unwinding panic"). Explain it and
                // exit cleanly instead.
                Err(error) => show_startup_error(app.handle(), &root, &error.message),
            }
            Ok(())
        })
        .invoke_handler(commands::handler())
        .run(tauri::generate_context!())
        .expect("Folio could not start. Check that the application data directory is writable.");
}

/// Shows why Folio could not open its local data, then quits when the
/// message is dismissed. The main window is hidden (not closed) so the app
/// stays alive until the user has read the message.
#[cfg(feature = "desktop")]
fn show_startup_error(app: &tauri::AppHandle, data_dir: &std::path::Path, reason: &str) {
    use tauri::Manager;
    use tauri_plugin_dialog::{DialogExt, MessageDialogButtons, MessageDialogKind};

    eprintln!(
        "Folio could not start: {reason} (data folder: {})",
        data_dir.display()
    );
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.hide();
    }
    let handle = app.clone();
    app.dialog()
        .message(format!(
            "{reason}\n\nFolio's data folder is:\n{}\n\nNothing was changed or deleted.",
            data_dir.display()
        ))
        .title("Folio could not start")
        .kind(MessageDialogKind::Error)
        .buttons(MessageDialogButtons::Ok)
        .show(move |_| handle.exit(1));
}
