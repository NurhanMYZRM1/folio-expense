//! Opt-in platform checks. Not compiled into desktop bundles. Never prints credentials.
use base64::{engine::general_purpose::STANDARD, Engine};
use folio::{
    domain::settings::Settings,
    security::secrets::{verification_store, OsSecretStore, SecretStore},
    services::extraction::{online::OnlineVisionExtractor, ExtractionInput, ReceiptExtractor},
};
use sha2::{Digest, Sha256};
use std::{env, process::Command};
use zeroize::Zeroizing;

type CheckResult = Result<(), Box<dyn std::error::Error>>;

fn keychain() -> CheckResult {
    let id = uuid::Uuid::new_v4().to_string();
    let vault = verification_store(&id)?;
    let result = (|| -> CheckResult {
        if vault.get()?.is_some() {
            return Err("Verification entry unexpectedly exists.".into());
        }
        let value = Zeroizing::new(format!("folio-test-{}", uuid::Uuid::new_v4()));
        vault.set(&value)?;
        if vault.get()?.as_deref() != Some(&value) {
            return Err("Credential round trip failed.".into());
        }
        let digest = format!("{:x}", Sha256::digest(value.as_bytes()));
        let child = Command::new(env::current_exe()?)
            .args(["keychain-read", &id, &digest])
            .status()?;
        if !child.success() {
            return Err("Credential did not persist across processes.".into());
        }
        vault.set("folio-test-replacement")?;
        if vault.get()?.as_deref().map(|s| s.as_str()) != Some("folio-test-replacement") {
            return Err("Credential replacement failed.".into());
        }
        Ok(())
    })();
    let cleanup = vault.delete();
    result?;
    cleanup?;
    if vault.get()?.is_some() {
        return Err("Credential deletion failed.".into());
    }
    vault.delete()?; // Deleting an absent entry is intentionally idempotent.
    println!("PASS: OS vault create/read/cross-process persistence/replace/delete; temporary entry removed.");
    Ok(())
}

/// Reads the provider endpoint and model saved in Folio's Settings, so the check exercises
/// whichever OpenAI-compatible provider the user configured. Mirrors Tauri's `app_data_dir`.
fn saved_provider() -> Result<(String, String), Box<dyn std::error::Error>> {
    let data_dir = if cfg!(windows) {
        std::path::PathBuf::from(env::var("APPDATA")?)
    } else {
        std::path::PathBuf::from(env::var("HOME")?).join("Library/Application Support")
    };
    let db = data_dir.join("com.folio.expenses/expense-app/database/expenses.sqlite");
    let conn =
        rusqlite::Connection::open_with_flags(&db, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    let settings = conn
        .query_row(
            "SELECT value FROM settings WHERE key='preferences'",
            [],
            |r| r.get::<_, String>(0),
        )
        .ok()
        .map(|v| serde_json::from_str::<Settings>(&v))
        .transpose()?
        .unwrap_or_default();
    Ok((settings.api_base_url, settings.ai_model))
}

fn ai(image_path: &str) -> CheckResult {
    let key = OsSecretStore
        .get()?
        .ok_or("No Folio API credential is configured.")?;
    let (base_url, model) = saved_provider()?;
    println!("Provider: {base_url} · model {model}");
    let bytes = std::fs::read(image_path)?;
    if !bytes.starts_with(b"\x89PNG\r\n\x1a\n") || bytes.len() > 2 * 1024 * 1024 {
        return Err("Use the synthetic PNG test receipt (maximum 2 MB).".into());
    }
    let images = vec![format!("data:image/png;base64,{}", STANDARD.encode(bytes))];
    let extractor = OnlineVisionExtractor {
        base_url: &base_url,
        model: &model,
        credential: &key,
    };
    let result = extractor.extract(&ExtractionInput {
        raw_text: "",
        images: &images,
        default_currency: "MYR",
        ocr_confidence: None,
    })?;
    if result.total_amount_minor != Some(8450) || result.currency.as_deref() != Some("MYR") {
        return Err(
            "Live extraction returned an unexpected synthetic receipt total/currency.".into(),
        );
    }
    println!("PASS: live vision provider returned validated structured extraction, MYR 84.50.");
    Ok(())
}

fn main() -> CheckResult {
    let args: Vec<_> = env::args().collect();
    match args.get(1).map(String::as_str) {
        Some("keychain") => keychain(),
        Some("keychain-read") => {
            let vault = verification_store(args.get(2).ok_or("Missing verification ID")?)?;
            let value = vault.get()?.ok_or("Verification entry not found")?;
            let actual = format!("{:x}", Sha256::digest(value.as_bytes()));
            if Some(&actual) != args.get(3) {
                return Err("Cross-process credential did not match.".into());
            }
            Ok(())
        }
        Some("ai") => ai(args.get(2).ok_or("Provide a synthetic receipt PNG path")?),
        _ => Err("Usage: platform-verify keychain | ai <synthetic-receipt.png>".into()),
    }
}
