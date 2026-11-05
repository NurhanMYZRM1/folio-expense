//! Android JNI entry points. The native module owns the Android Keystore;
//! financial commands still use exactly the same guarded C ABI as iOS.
use crate::{
    error::{AppError, Result},
    ffi,
};
use jni::{
    objects::{GlobalRef, JClass, JObject, JString, JValue},
    sys::jstring,
    JNIEnv, JavaVM,
};
use std::{
    ffi::{CStr, CString},
    sync::OnceLock,
};
use zeroize::Zeroizing;

static VM: OnceLock<JavaVM> = OnceLock::new();
static VAULT: OnceLock<GlobalRef> = OnceLock::new();

fn credential_error() -> AppError {
    AppError::new(
        "CredentialError",
        "The Android credential store is unavailable. Offline extraction remains available.",
    )
}
fn with_vault<T>(
    f: impl FnOnce(&mut JNIEnv<'_>, &JClass<'_>) -> jni::errors::Result<T>,
) -> Result<T> {
    let mut env = VM
        .get()
        .ok_or_else(credential_error)?
        .attach_current_thread()
        .map_err(|_| credential_error())?;
    let class: &JClass = VAULT.get().ok_or_else(credential_error)?.as_obj().into();
    let result = f(&mut env, class);
    // Never leave a Java exception pending across the Rust command boundary.
    if env.exception_check().unwrap_or(true) {
        let _ = env.exception_clear();
        return Err(credential_error());
    }
    result.map_err(|_| credential_error())
}
pub fn get_secret() -> Result<Option<Zeroizing<String>>> {
    with_vault(|env, class| {
        let value = env
            .call_static_method(class, "readSecret", "()Ljava/lang/String;", &[])?
            .l()?;
        if value.is_null() {
            return Ok(None);
        }
        let value = JString::from(value);
        let secret: String = env.get_string(&value)?.into();
        Ok(Some(Zeroizing::new(secret)))
    })
}
pub fn set_secret(value: &str) -> Result<()> {
    with_vault(|env, class| {
        let value = JObject::from(env.new_string(value)?);
        env.call_static_method(
            class,
            "writeSecret",
            "(Ljava/lang/String;)V",
            &[JValue::Object(&value)],
        )?;
        Ok(())
    })
}
pub fn delete_secret() -> Result<()> {
    with_vault(|env, class| {
        env.call_static_method(class, "deleteSecret", "()V", &[])?;
        Ok(())
    })
}
fn take(env: &mut JNIEnv<'_>, ptr: *mut std::ffi::c_char) -> jstring {
    // SAFETY: only guarded C ABI results, whose allocation is freed below.
    let response = unsafe { CStr::from_ptr(ptr) }
        .to_string_lossy()
        .into_owned();
    unsafe { ffi::folio_string_free(ptr) };
    env.new_string(response)
        .map(|s| s.into_raw())
        .unwrap_or(std::ptr::null_mut())
}
fn argument(env: &mut JNIEnv<'_>, s: &JString<'_>) -> Option<CString> {
    let s: String = env.get_string(s).ok()?.into();
    CString::new(s).ok()
}
#[no_mangle]
pub extern "system" fn Java_expo_modules_foliocore_NativeCore_open(
    mut env: JNIEnv,
    class: JClass,
    root: JString,
) -> jstring {
    if let Ok(vm) = env.get_java_vm() {
        let _ = VM.set(vm);
    }
    if let Ok(class) = env.new_global_ref(class) {
        let _ = VAULT.set(class);
    }
    let root = argument(&mut env, &root);
    let response =
        unsafe { ffi::folio_open(root.as_ref().map_or(std::ptr::null(), |s| s.as_ptr())) };
    take(&mut env, response)
}
#[no_mangle]
pub extern "system" fn Java_expo_modules_foliocore_NativeCore_invoke(
    mut env: JNIEnv,
    _class: JClass,
    command: JString,
    args: JString,
) -> jstring {
    let command = argument(&mut env, &command);
    let args = argument(&mut env, &args);
    let response = unsafe {
        ffi::folio_invoke(
            command.as_ref().map_or(std::ptr::null(), |s| s.as_ptr()),
            args.as_ref().map_or(std::ptr::null(), |s| s.as_ptr()),
        )
    };
    take(&mut env, response)
}
