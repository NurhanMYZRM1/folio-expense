use crate::error::{AppError, Result};
use rusqlite::Connection;
use std::{path::Path, time::Duration};
/// The newest database layout this build understands. A new migration must
/// stamp this value; the tests check the two agree.
pub const SCHEMA_VERSION: i32 = 1;
pub fn open(path: &Path) -> Result<Connection> {
    let mut db = Connection::open(path)?;
    db.busy_timeout(Duration::from_secs(5))?;
    // Check the version before anything that writes (switching to WAL
    // rewrites the file header), so a newer database is left untouched.
    check_version(&db)?;
    db.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON; PRAGMA synchronous=FULL; PRAGMA trusted_schema=OFF;")?;
    migrate(&mut db)?;
    Ok(db)
}
/// Reads the schema version, refusing a database from a newer Folio.
fn check_version(db: &Connection) -> Result<i32> {
    let version: i32 = db.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    if version > SCHEMA_VERSION {
        return Err(AppError::new(
            "DatabaseVersion",
            format!(
                "This database was created by a newer version of Folio (database version {version}; \
                 this Folio supports up to version {SCHEMA_VERSION}). Update Folio, or move the data \
                 folder aside to start with a new, empty database."
            ),
        ));
    }
    Ok(version)
}
pub fn migrate(db: &mut Connection) -> Result<()> {
    let version = check_version(db)?;
    if version < 1 {
        let tx = db.transaction()?;
        tx.execute_batch(include_str!("../../migrations/001_initial.sql"))?;
        tx.commit()?;
    }
    Ok(())
}
