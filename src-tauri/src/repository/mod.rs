pub mod claim_repository;
pub mod expense_repository;
pub mod receipt_repository;
use crate::{
    domain::{id, now},
    error::Result,
};
use rusqlite::{params, Connection};
pub fn audit(db: &Connection, event: &str, entity: &str, details: serde_json::Value) -> Result<()> {
    db.execute("INSERT INTO audit_events(id,event_type,entity_id,details,created_at) VALUES(?1,?2,?3,?4,?5)",params![id(),event,entity,details.to_string(),now()])?;
    Ok(())
}
pub fn enqueue(db: &Connection, job_type: &str, entity: &str, payload: &str) -> Result<String> {
    let job_id = id();
    let time = now();
    db.execute("INSERT INTO jobs(id,job_type,entity_id,payload,status,created_at,updated_at) VALUES(?1,?2,?3,?4,'pending',?5,?5)",params![job_id,job_type,entity,payload,time])?;
    Ok(job_id)
}
