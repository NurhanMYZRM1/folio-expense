//! Explicit local data export. VACUUM INTO includes committed WAL data and
//! removes freed pages; copying only the live .sqlite file would lose writes.
use super::AppService;
use crate::{
    db::SCHEMA_VERSION,
    domain::{id, now},
    error::{AppError, Result},
};
use base64::{engine::general_purpose::STANDARD, Engine};
use rusqlite::{types::ValueRef, Connection};
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use std::{fs, io::Write, path::PathBuf};
use ts_rs::TS;
use zip::{write::SimpleFileOptions, ZipWriter};

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct DataExport {
    pub id: String,
    pub file_name: String,
    pub path: String,
    pub files: u32,
}

struct Staging(PathBuf);
impl Drop for Staging {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn zip_error(_: zip::result::ZipError) -> AppError {
    AppError::new(
        "ExportError",
        "Unable to create the data archive. Check free space and try again.",
    )
}
fn options() -> SimpleFileOptions {
    SimpleFileOptions::default().unix_permissions(0o600)
}
/// Export SQL values without numeric coercion or reparsing JSON-valued text.
/// Each JSON row keeps its original column names and integer minor units.
fn write_records(db: &Connection, zip: &mut ZipWriter<fs::File>) -> Result<()> {
    zip.start_file("records.json", options())
        .map_err(zip_error)?;
    write!(
        zip,
        "{{\"formatVersion\":1,\"schemaVersion\":{SCHEMA_VERSION},\"tables\":{{"
    )?;
    let mut stmt = db.prepare("SELECT name FROM sqlite_schema WHERE type='table' AND name NOT LIKE 'sqlite_%' ORDER BY name")?;
    let names = stmt
        .query_map([], |r| r.get::<_, String>(0))?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    for (table_index, table) in names.iter().enumerate() {
        if table_index > 0 {
            zip.write_all(b",")?;
        }
        serde_json::to_writer(&mut *zip, table)?;
        zip.write_all(b":[")?;
        let mut q = db.prepare(&format!("SELECT * FROM \"{}\"", table.replace('"', "\"\"")))?;
        let columns: Vec<_> = q.column_names().iter().map(|s| s.to_string()).collect();
        let mut rows = q.query([])?;
        let mut first = true;
        while let Some(row) = rows.next()? {
            let mut object = Map::new();
            for (i, column) in columns.iter().enumerate() {
                let value = match row.get_ref(i)? {
                    ValueRef::Null => Value::Null,
                    ValueRef::Integer(n) => json!(n),
                    ValueRef::Real(n) => json!(n),
                    ValueRef::Text(s) => Value::String(
                        std::str::from_utf8(s)
                            .map_err(|_| AppError::invalid("The database contains invalid text."))?
                            .into(),
                    ),
                    ValueRef::Blob(b) => json!({"base64": STANDARD.encode(b)}),
                };
                object.insert(column.clone(), value);
            }
            if !first {
                zip.write_all(b",")?;
            }
            serde_json::to_writer(&mut *zip, &object)?;
            first = false;
        }
        zip.write_all(b"]")?;
    }
    zip.write_all(b"}}")?;
    Ok(())
}
impl AppService {
    /// Hold the service lock until originals are copied, so a concurrent delete
    /// cannot remove a receipt that the snapshot still references. Work runs on
    /// the native blocking pool, not the UI thread. No network or vault access.
    pub fn export_all_data(&self) -> Result<DataExport> {
        let db = self.conn()?;
        let export_id = id();
        let staging_path = self
            .paths
            .resolve(&format!("cache/data-export-{export_id}"))?;
        fs::create_dir(&staging_path)?;
        let staging = Staging(staging_path);
        let snapshot = staging.0.join("expenses.sqlite");
        // https://www.sqlite.org/lang_vacuum.html#vacuum_with_an_into_clause
        db.execute("VACUUM INTO ?1", [snapshot.to_string_lossy().as_ref()])?;
        let snapshot_db =
            Connection::open_with_flags(&snapshot, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)?;
        let mut q = snapshot_db.prepare("SELECT relative_path FROM receipt_files")?;
        for relative in q.query_map([], |r| r.get::<_, String>(0))? {
            if !self.paths.resolve(&relative?)?.is_file() {
                return Err(AppError::new(
                    "ExportError",
                    "An original receipt is missing. Restore it before exporting all data.",
                ));
            }
        }
        let temp_archive = staging.0.join("archive.zip");
        let mut zip = ZipWriter::new(
            fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&temp_archive)?,
        );
        zip.start_file("database/expenses.sqlite", options())
            .map_err(zip_error)?;
        std::io::copy(&mut fs::File::open(&snapshot)?, &mut zip)?;
        write_records(&snapshot_db, &mut zip)?;
        zip.start_file("README.txt", options()).map_err(zip_error)?;
        zip.write_all(b"Folio local data export, format 1.\n\nrecords.json contains all database tables and rows, using database column names. JSON-valued text columns remain text; BLOBs use {base64: ...}. Monetary values remain integer minor units.\n\nThe database snapshot includes committed changes from the WAL. Receipts and existing claim, invoice, CSV and XLSX exports are included. Logs, cache files, OS-vault credentials and previous full-data archives are excluded.\n\nThis ZIP is not encrypted. Store it in a protected location.\n\nTo restore: quit Folio, keep a copy of the current data folder, then extract database/, receipts/ and exports/ into an empty Folio data folder. Do not overlay a running or existing database or its WAL files. Use this version of Folio or newer. Credentials must be configured again; no account is connected by restoring this archive.\n")?;
        zip.start_file("manifest.json", options())
            .map_err(zip_error)?;
        serde_json::to_writer(
            &mut zip,
            &json!({
                "formatVersion": 1, "schemaVersion": SCHEMA_VERSION,
                "createdAt": now(), "appVersion": env!("CARGO_PKG_VERSION"),
                "encrypted": false,
            }),
        )?;
        let mut files = 4;
        for relative in [
            "receipts",
            "exports/claims",
            "exports/csv",
            "exports/xlsx",
            "exports/invoices",
        ] {
            self.archive_directory(relative, &mut zip, &mut files)?;
        }
        zip.finish().map_err(zip_error)?.sync_all()?;
        let destination = self
            .paths
            .resolve(&format!("exports/data/{export_id}.zip"))?;
        fs::create_dir_all(destination.parent().unwrap())?;
        fs::rename(&temp_archive, &destination)?;
        Ok(DataExport {
            id: export_id,
            file_name: "Folio-data.zip".into(),
            path: destination.to_string_lossy().into(),
            files,
        })
    }
    fn archive_directory(
        &self,
        relative: &str,
        zip: &mut ZipWriter<fs::File>,
        files: &mut u32,
    ) -> Result<()> {
        let path = self.paths.resolve(relative)?;
        let metadata = match fs::symlink_metadata(&path) {
            Ok(m) => m,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(e) => return Err(e.into()),
        };
        if metadata.file_type().is_symlink() || !metadata.is_dir() {
            return Err(AppError::invalid(
                "Data export cannot include linked folders or special files.",
            ));
        }
        let mut entries = fs::read_dir(path)?.collect::<std::io::Result<Vec<_>>>()?;
        entries.sort_by_key(|e| e.file_name());
        for entry in entries {
            let name = entry
                .file_name()
                .into_string()
                .map_err(|_| AppError::invalid("A data filename is not valid UTF-8."))?;
            let child = format!("{relative}/{name}");
            let kind = entry.file_type()?;
            if kind.is_dir() {
                self.archive_directory(&child, zip, files)?;
            } else if kind.is_file() {
                let path = self.paths.resolve(&child)?;
                zip.start_file(&child, options()).map_err(zip_error)?;
                std::io::copy(&mut fs::File::open(path)?, zip)?;
                *files += 1;
            } else {
                return Err(AppError::invalid(
                    "Data export cannot include links or special files.",
                ));
            }
        }
        Ok(())
    }
    pub fn data_export_path(&self, id: &str) -> Result<String> {
        uuid::Uuid::parse_str(id).map_err(|_| AppError::invalid("Invalid export ID."))?;
        let path = self.paths.resolve(&format!("exports/data/{id}.zip"))?;
        if !path.is_file() {
            return Err(AppError::not_found());
        }
        Ok(path.to_string_lossy().into())
    }
}
