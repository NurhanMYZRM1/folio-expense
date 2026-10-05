#[cfg(feature = "desktop")]
mod commands;
pub mod db;
pub mod dispatch;
pub mod domain;
pub mod error;
#[cfg(feature = "mobile")]
pub mod ffi;
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
        // Setup must never return an error: on macOS Tauri turns it into an
        // abort ("non-unwinding panic"). Failures are explained in a dialog
        // and the app exits cleanly instead.
        .setup(|app| {
            let root = match app.path().app_data_dir() {
                Ok(dir) => dir.join("expense-app"),
                Err(error) => {
                    let reason = format!("Folio could not find its data folder ({error}).");
                    show_startup_error(app.handle(), &reason, None, true);
                    return Ok(());
                }
            };
            match services::AppService::open(
                root.clone(),
                std::sync::Arc::new(security::secrets::OsSecretStore),
            ) {
                Ok(service) => {
                    app.manage(std::sync::Arc::new(service));
                }
                Err(error) => {
                    // A newer database is refused before anything is written.
                    let untouched = error.code == "DatabaseVersion";
                    show_startup_error(app.handle(), &error.message, Some(&root), untouched);
                }
            }
            Ok(())
        })
        .invoke_handler(commands::handler())
        .run(tauri::generate_context!())
        .expect("Folio could not start. Check that the application data directory is writable.");
}

/// Shows why Folio could not open its local data, then quits when the
/// message is dismissed. The main window is hidden (not closed) so the app
/// stays alive until the user has read the message. `untouched` is true only
/// when the failure happened before anything on disk was written.
#[cfg(feature = "desktop")]
fn show_startup_error(
    app: &tauri::AppHandle,
    reason: &str,
    data_dir: Option<&std::path::Path>,
    untouched: bool,
) {
    use tauri::Manager;
    use tauri_plugin_dialog::{DialogExt, MessageDialogButtons, MessageDialogKind};

    let mut message = reason.to_string();
    if let Some(dir) = data_dir {
        message.push_str(&format!("\n\nFolio's data folder is:\n{}", dir.display()));
    }
    if untouched {
        message.push_str("\n\nNothing was changed or deleted.");
    }
    eprintln!("Folio could not start: {}", message.replace('\n', " "));
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.hide();
    }
    let handle = app.clone();
    app.dialog()
        .message(message)
        .title("Folio could not start")
        .kind(MessageDialogKind::Error)
        .buttons(MessageDialogButtons::Ok)
        .show(move |_| handle.exit(1));
}
