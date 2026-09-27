use crate::error::{AppError, Result};
use rusqlite::Connection;
use std::{path::Path, time::Duration};
pub fn open(path: &Path) -> Result<Connection> {
    let mut db = Connection::open(path)?;
    db.busy_timeout(Duration::from_secs(5))?;
    db.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON; PRAGMA synchronous=FULL; PRAGMA trusted_schema=OFF;")?;
    migrate(&mut db)?;
    Ok(db)
}
pub fn migrate(db: &mut Connection) -> Result<()> {
    let version: i32 = db.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    if version > 1 {
        return Err(AppError::new(
            "DatabaseVersion",
            "This database was created by a newer version of Folio.",
        ));
    }
    if version < 1 {
        let tx = db.transaction()?;
        tx.execute_batch(include_str!("../../migrations/001_initial.sql"))?;
        tx.commit()?;
    }
    Ok(())
}
