//! Test-only stdio adapter. Never included in desktop packages and never opens a network port.
use folio::{
    dispatch::dispatch,
    error::{AppError, Result},
    security::secrets::SecretStore,
    services::AppService,
};
use serde_json::{json, Value};
use std::{
    io::{self, BufRead, Write},
    sync::Arc,
};
use zeroize::Zeroizing;
struct NoSecrets;
impl SecretStore for NoSecrets {
    fn get(&self) -> Result<Option<Zeroizing<String>>> {
        Ok(None)
    }
    fn set(&self, _: &str) -> Result<()> {
        Err(AppError::invalid(
            "Credentials are disabled in the test driver.",
        ))
    }
    fn delete(&self) -> Result<()> {
        Ok(())
    }
}
fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
    let root = std::env::args()
        .nth(1)
        .ok_or("Expected isolated test data directory")?;
    let s = AppService::open(root.into(), Arc::new(NoSecrets))?;
    for line in io::stdin().lock().lines() {
        let line = line?;
        let request: Value = serde_json::from_str(&line)?;
        let request_id = &request["requestId"];
        let output = match dispatch(
            &s,
            request["command"].as_str().unwrap_or(""),
            request["args"].clone(),
        ) {
            Ok(result) => json!({"requestId":request_id,"result":result}),
            Err(error) => json!({"requestId":request_id,"error":error}),
        };
        println!("{output}");
        io::stdout().flush()?;
    }
    Ok(())
}
