use super::AppService;
use crate::{
    domain::{
        id, now,
        receipt::{ImportOutcome, ReceiptContent},
    },
    error::{AppError, Result},
    repository::{self, receipt_repository as receipts},
    storage::receipt_storage,
};
use base64::{engine::general_purpose::STANDARD, Engine};
use rusqlite::params;
use std::{fs, path::Path};
impl AppService {
    pub fn import_receipts(&self, paths: Vec<String>) -> Result<Vec<ImportOutcome>> {
        if paths.len() > 100 {
            return Err(AppError::invalid("Import up to 100 receipts at a time."));
        }
        Ok(paths
            .into_iter()
            .map(|p| {
                let path = Path::new(&p);
                let filename = path
                    .file_name()
                    .map(|f| f.to_string_lossy().to_string())
                    .unwrap_or_default();
                match self.import_receipt(path) {
                    Ok(id) => ImportOutcome {
                        filename,
                        expense_id: Some(id),
                        error: None,
                    },
                    Err(error) => ImportOutcome {
                        filename,
                        expense_id: None,
                        error: Some(error),
                    },
                }
            })
            .collect())
    }
    pub fn import_receipt(&self, path: &Path) -> Result<String> {
        let (receipt, temp) = receipt_storage::stage(&self.paths, path)?;
        let mut db = self.conn()?;
        if let Some(existing) = receipts::duplicate(&db, &receipt.sha256)? {
            let _ = fs::remove_file(&temp);
            return Err(AppError {
                code: "DuplicateReceipt".into(),
                message: "This receipt appears to have already been imported.".into(),
                existing_expense_id: Some(existing),
            });
        }
        let expense_id = id();
        let time = now();
        let result = (|| -> Result<()> {
            receipt_storage::publish(&self.paths, &receipt, &temp)?;
            let tx = db.transaction()?;
            receipts::insert(&tx, &receipt)?;
            tx.execute("INSERT INTO expenses(id,receipt_id,status,created_at,updated_at) VALUES(?1,?2,'draft',?3,?3)",params![expense_id,receipt.id,time])?;
            repository::audit(
                &tx,
                "receipt.imported",
                &receipt.id,
                serde_json::json!({"sha256":receipt.sha256}),
            )?;
            repository::audit(
                &tx,
                "expense.created",
                &expense_id,
                serde_json::json!({"receiptId":receipt.id}),
            )?;
            // Durable jobs are inserted with the receipt transaction. Workers only see them after commit.
            repository::enqueue(&tx, "generate_thumbnail", &expense_id, "{}")?;
            repository::enqueue(&tx, "extract_receipt", &expense_id, "{}")?;
            tx.commit()?;
            Ok(())
        })();
        if let Err(e) = result {
            let _ = fs::remove_file(&temp);
            if let Ok(p) = self.paths.resolve(&receipt.relative_path) {
                let _ = fs::remove_file(&p);
                if let Some(parent) = p.parent() {
                    let _ = fs::remove_dir(parent);
                }
            }
            return Err(e);
        }
        Ok(expense_id)
    }
    pub fn read_receipt(&self, id: &str) -> Result<ReceiptContent> {
        let receipt = receipts::get(&*self.conn()?, id)?;
        let bytes = fs::read(self.paths.resolve(&receipt.relative_path)?)?;
        Ok(ReceiptContent {
            receipt,
            base64: STANDARD.encode(bytes),
        })
    }
    pub fn read_thumbnail(&self, id: &str) -> Result<Option<String>> {
        receipts::get(&*self.conn()?, id)?;
        let p = self.paths.resolve(&format!("receipts/{id}/preview.webp"))?;
        if !p.exists() {
            return Ok(None);
        }
        Ok(Some(format!(
            "data:image/webp;base64,{}",
            STANDARD.encode(fs::read(p)?)
        )))
    }
    pub(crate) fn recover_storage(&self) -> Result<()> {
        let db = self.conn()?;
        for entry in fs::read_dir(self.paths.root.join("cache"))? {
            let path = entry?.path();
            if path.extension().is_some_and(|v| v == "part") {
                let _ = fs::remove_file(path);
            }
        }
        for entry in fs::read_dir(self.paths.root.join("receipts"))? {
            let entry = entry?;
            if !entry.file_type()?.is_dir() {
                continue;
            }
            let name = entry.file_name().to_string_lossy().to_string();
            if uuid::Uuid::parse_str(&name).is_err() {
                continue;
            }
            let exists: bool = db.query_row(
                "SELECT EXISTS(SELECT 1 FROM receipt_files WHERE id=?1)",
                [&name],
                |r| r.get(0),
            )?;
            if !exists {
                // Only generated orphan files are eligible for cleanup; never follow links or delete arbitrary directories.
                for ext in ["png", "jpg", "pdf"] {
                    let _ = fs::remove_file(entry.path().join(format!("original.{ext}")));
                }
                let _ = fs::remove_file(entry.path().join("preview.webp"));
                let _ = fs::remove_dir(entry.path());
            }
        }
        Ok(())
    }
}
