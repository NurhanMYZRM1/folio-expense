//! C ABI for the mobile app (Expo native module). Requests and responses are
//! the same JSON the test driver speaks: `{"result": …}` or `{"error": AppError}`.
use crate::{
    dispatch::dispatch, error::AppError, security::secrets::OsSecretStore, services::AppService,
};
use serde_json::{json, Value};
use std::{
    ffi::{c_char, CStr, CString},
    panic::{catch_unwind, AssertUnwindSafe},
    path::PathBuf,
    sync::{Arc, OnceLock},
};

static SERVICE: OnceLock<Arc<AppService>> = OnceLock::new();

fn read(ptr: *const c_char) -> Option<String> {
    if ptr.is_null() {
        return None;
    }
    // SAFETY: callers pass NUL-terminated UTF-8 strings that outlive this call.
    unsafe { CStr::from_ptr(ptr) }
        .to_str()
        .ok()
        .map(str::to_owned)
}

fn respond(value: Value) -> *mut c_char {
    CString::new(value.to_string())
        .unwrap_or_else(|_| {
            CString::new(r#"{"error":{"code":"InternalError","message":"Invalid response."}}"#)
                .unwrap()
        })
        .into_raw()
}

fn error(e: AppError) -> Value {
    json!({ "error": e })
}

fn guarded(f: impl FnOnce() -> Value) -> *mut c_char {
    respond(catch_unwind(AssertUnwindSafe(f)).unwrap_or_else(|_| {
        error(AppError::new(
            "InternalError",
            "Folio hit an unexpected problem. Saved receipts remain available.",
        ))
    }))
}

/// Opens the workspace under `root` (the app's data folder). Safe to call again.
///
/// # Safety
/// `root` must be null or a NUL-terminated string valid for the duration of the call.
#[no_mangle]
pub unsafe extern "C" fn folio_open(root: *const c_char) -> *mut c_char {
    guarded(|| {
        if SERVICE.get().is_some() {
            return json!({ "result": null });
        }
        let Some(root) = read(root) else {
            return error(AppError::invalid("Missing data folder."));
        };
        match AppService::open(PathBuf::from(root), Arc::new(OsSecretStore)) {
            Ok(service) => {
                let _ = SERVICE.set(Arc::new(service));
                json!({ "result": null })
            }
            Err(e) => error(e),
        }
    })
}

/// Runs one command. Blocking; call it off the main thread.
///
/// # Safety
/// `command` and `args` must each be null or a NUL-terminated string valid for
/// the duration of the call.
#[no_mangle]
pub unsafe extern "C" fn folio_invoke(command: *const c_char, args: *const c_char) -> *mut c_char {
    guarded(|| {
        let Some(service) = SERVICE.get() else {
            return error(AppError::new(
                "InternalError",
                "The workspace isn't open yet.",
            ));
        };
        let command = read(command).unwrap_or_default();
        let args = match read(args).map(|a| serde_json::from_str::<Value>(&a)) {
            Some(Ok(v)) => v,
            Some(Err(e)) => return error(e.into()),
            None => Value::Null,
        };
        match dispatch(service, &command, args) {
            Ok(result) => json!({ "result": result }),
            Err(e) => error(e),
        }
    })
}

/// Frees a string returned by `folio_open` or `folio_invoke`.
///
/// # Safety
/// `ptr` must be null or a pointer returned by `folio_open`/`folio_invoke`
/// that has not been freed yet.
#[no_mangle]
pub unsafe extern "C" fn folio_string_free(ptr: *mut c_char) {
    if !ptr.is_null() {
        // SAFETY: `ptr` came from `CString::into_raw` in this module.
        drop(unsafe { CString::from_raw(ptr) });
    }
}
