use crate::{
    domain::receipt::ReceiptFile,
    error::{AppError, Result},
};
use rusqlite::{params, Connection, OptionalExtension};
pub fn get(db: &Connection, id: &str) -> Result<ReceiptFile> {
    db.query_row("SELECT id,sha256,original_filename,mime_type,relative_path,size_bytes,created_at FROM receipt_files WHERE id=?1",[id],|r|Ok(ReceiptFile{id:r.get(0)?,sha256:r.get(1)?,original_filename:r.get(2)?,mime_type:r.get(3)?,relative_path:r.get(4)?,size_bytes:r.get(5)?,created_at:r.get(6)?})).optional()?.ok_or_else(AppError::not_found)
}
pub fn duplicate(db: &Connection, hash: &str) -> Result<Option<String>> {
    Ok(db.query_row("SELECT e.id FROM receipt_files r JOIN expenses e ON e.receipt_id=r.id WHERE r.sha256=?1",[hash],|r|r.get(0)).optional()?)
}
pub fn insert(db: &Connection, r: &ReceiptFile) -> Result<()> {
    db.execute("INSERT INTO receipt_files(id,sha256,original_filename,mime_type,relative_path,size_bytes,created_at,updated_at) VALUES(?1,?2,?3,?4,?5,?6,?7,?7)",params![r.id,r.sha256,r.original_filename,r.mime_type,r.relative_path,r.size_bytes,r.created_at])?;
    Ok(())
}
