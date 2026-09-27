use crate::{
    domain::{
        claim::ExportSnapshot,
        expense::{validate_values, ExpenseEdit, ExpenseStatus},
        extraction::{Extraction, FieldMeta, FieldSource, Job, JobLease},
        id, now,
    },
    error::{AppError, Result},
    repository::{self, expense_repository as expenses},
    services::{
        extraction::{
            normalizer, offline::LocalOcrExtractor, online::OnlineVisionExtractor, ExtractionInput,
            ReceiptExtractor,
        },
        AppService,
    },
};
use base64::{engine::general_purpose::STANDARD, Engine};
use rusqlite::{params, Connection, OptionalExtension};
use std::{fs, io::Write};
fn job_row(r: &rusqlite::Row) -> rusqlite::Result<Job> {
    Ok(Job {
        id: r.get(0)?,
        job_type: r.get(1)?,
        entity_id: r.get(2)?,
        status: r.get(3)?,
        attempts: r.get(4)?,
        last_error: r.get(5)?,
        created_at: r.get(6)?,
        updated_at: r.get(7)?,
    })
}
const JOB_SELECT: &str =
    "SELECT id,job_type,entity_id,status,attempts,last_error,created_at,updated_at FROM jobs";
fn lease(db: &Connection, job_id: &str, token: &str, kind: Option<&str>) -> Result<Job> {
    let job = db
        .query_row(
            &format!("{JOB_SELECT} WHERE id=?1 AND lease_token=?2 AND status='running'"),
            params![job_id, token],
            job_row,
        )
        .optional()?
        .ok_or_else(|| {
            AppError::new(
                "StaleJob",
                "This background job has already completed or restarted.",
            )
        })?;
    if kind.is_some_and(|k| k != job.job_type) {
        return Err(AppError::invalid("Unexpected job type."));
    }
    Ok(job)
}
impl AppService {
    pub fn jobs(&self) -> Result<Vec<Job>> {
        let db = self.conn()?;
        // Keep every report and actionable job reachable when the recent receipt queue rolls over.
        let mut st = db.prepare(&format!("{JOB_SELECT} WHERE job_type='generate_pdf' OR status!='completed' OR id IN (SELECT id FROM jobs ORDER BY created_at DESC LIMIT 200) ORDER BY created_at DESC"))?;
        let rows = st
            .query_map([], job_row)?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }
    pub(crate) fn recover_jobs(&self) -> Result<()> {
        let db = self.conn()?;
        db.execute("UPDATE jobs SET status=CASE WHEN attempts>=3 THEN 'failed' ELSE 'pending' END,lease_token=NULL,last_error='Processing was interrupted. The local receipt is safe.',updated_at=?1 WHERE status='running'",[now()])?;
        db.execute("UPDATE expenses SET status='needs_review',version=version+1,updated_at=?1 WHERE status='extracting'",[now()])?;
        Ok(())
    }
    pub fn take_job(&self) -> Result<Option<JobLease>> {
        let mut db = self.conn()?;
        let tx = db.transaction()?;
        // Heartbeats also recover a crashed/reloaded renderer without requiring a desktop restart.
        tx.execute("UPDATE jobs SET status=CASE WHEN attempts>=3 THEN 'failed' ELSE 'pending' END,lease_token=NULL,last_error='Worker interrupted; retrying safely.' WHERE status='running' AND updated_at < ?1",[(chrono::Utc::now()-chrono::Duration::minutes(2)).to_rfc3339_opts(chrono::SecondsFormat::Millis,true)])?;
        tx.execute("UPDATE expenses SET status='needs_review',version=version+1,updated_at=?1 WHERE status='extracting' AND NOT EXISTS(SELECT 1 FROM jobs WHERE jobs.entity_id=expenses.id AND job_type='extract_receipt' AND status IN ('pending','running'))",[now()])?;
        let job=tx.query_row(&format!("{JOB_SELECT} WHERE status='pending' AND job_type!='future_sync' ORDER BY created_at,CASE job_type WHEN 'generate_thumbnail' THEN 0 ELSE 1 END,id LIMIT 1"),[],job_row).optional()?;
        let Some(mut job) = job else {
            tx.commit()?;
            return Ok(None);
        };
        let token = id();
        let time = now();
        tx.execute("UPDATE jobs SET status='running',lease_token=?1,attempts=attempts+1,updated_at=?2 WHERE id=?3",params![token,time,job.id])?;
        let receipt_id = if job.job_type != "generate_pdf" {
            let e = expenses::get(&tx, &job.entity_id)?;
            if job.job_type == "extract_receipt" {
                if matches!(e.status, ExpenseStatus::Submitted | ExpenseStatus::Archived) {
                    tx.execute("UPDATE jobs SET status='failed',last_error='Reopen this claim before extraction.',lease_token=NULL WHERE id=?1",[&job.id])?;
                    tx.commit()?;
                    return Ok(None);
                }
                if e.status != ExpenseStatus::Ready {
                    tx.execute("UPDATE expenses SET status='extracting',version=version+1,updated_at=?1 WHERE id=?2",params![time,e.id])?;
                }
                repository::audit(
                    &tx,
                    "extraction.started",
                    &e.id,
                    serde_json::json!({"jobId":job.id}),
                )?;
            }
            e.receipt_id
        } else {
            None
        };
        job.status = "running".into();
        job.attempts += 1;
        job.updated_at = time;
        tx.commit()?;
        Ok(Some(JobLease {
            job,
            token,
            receipt_id,
        }))
    }
    pub fn heartbeat(&self, id: &str, token: &str) -> Result<()> {
        let db = self.conn()?;
        lease(&db, id, token, None)?;
        db.execute(
            "UPDATE jobs SET updated_at=?1 WHERE id=?2",
            params![now(), id],
        )?;
        Ok(())
    }
    pub fn fail_job(&self, id: &str, token: &str, message: &str) -> Result<()> {
        let mut db = self.conn()?;
        let tx = db.transaction()?;
        let job = lease(&tx, id, token, None)?;
        let message: String = message.chars().take(500).collect();
        let time = now();
        tx.execute("UPDATE jobs SET status='failed',last_error=?1,lease_token=NULL,updated_at=?2 WHERE id=?3",params![message,time,id])?;
        if job.job_type == "extract_receipt" {
            tx.execute("UPDATE expenses SET status='needs_review',version=version+1,updated_at=?1 WHERE id=?2 AND status IN ('draft','extracting')",params![time,job.entity_id])?;
            tx.execute("INSERT INTO extraction_runs(id,expense_id,provider,error,created_at) VALUES(?1,?2,'local_ocr',?3,?4)",params![crate::domain::id(),job.entity_id,message,time])?;
            repository::audit(
                &tx,
                "extraction.failed",
                &job.entity_id,
                serde_json::json!({"jobId":job.id}),
            )?;
        }
        tx.commit()?;
        Ok(())
    }
    pub fn retry_job(&self, id: &str) -> Result<()> {
        let mut db = self.conn()?;
        let tx = db.transaction()?;
        let job = tx.query_row(&format!("{JOB_SELECT} WHERE id=?1"), [id], job_row)?;
        if job.status != "failed" {
            return Err(AppError::invalid("Only failed jobs can be retried."));
        }
        let active:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM jobs WHERE entity_id=?1 AND job_type=?2 AND status IN ('pending','running'))",params![job.entity_id,job.job_type],|r|r.get(0))?;
        if active {
            return Err(AppError::invalid("This receipt already has a queued job."));
        }
        tx.execute(
            "UPDATE jobs SET status='pending',attempts=0,last_error=NULL,updated_at=?1 WHERE id=?2",
            params![now(), id],
        )?;
        tx.commit()?;
        Ok(())
    }
    pub fn try_online(&self, id: &str, token: &str, images: Vec<String>) -> Result<bool> {
        {
            let db = self.conn()?;
            lease(&db, id, token, Some("extract_receipt"))?;
        }
        let settings = self.settings()?;
        if !settings.online_enabled {
            return Ok(false);
        }
        let credential = match self.secrets.get() {
            Ok(Some(v)) => v,
            _ => return Ok(false),
        };
        if images.is_empty()
            || images.len() > 30
            || images.iter().map(String::len).sum::<usize>() > 60 * 1024 * 1024
        {
            return Err(AppError::invalid("Too much image data for extraction."));
        }
        for image in &images {
            if !image.starts_with("data:image/png;base64,")
                && !image.starts_with("data:image/jpeg;base64,")
            {
                return Err(AppError::invalid("Invalid image input."));
            }
        }
        let extractor = OnlineVisionExtractor {
            base_url: &settings.api_base_url,
            model: &settings.ai_model,
            credential: &credential,
        };
        match extractor.extract(&ExtractionInput {
            raw_text: "",
            images: &images,
            default_currency: &settings.default_currency,
        }) {
            Ok(result) => {
                self.complete_extraction(id, token, result, FieldSource::OnlineAi, None)?;
                Ok(true)
            }
            Err(error) => {
                let mut db = self.conn()?;
                let tx = db.transaction()?;
                let job = lease(&tx, id, token, None)?;
                tx.execute("INSERT INTO extraction_runs(id,expense_id,provider,error,created_at) VALUES(?1,?2,'online_ai',?3,?4)",params![crate::domain::id(),job.entity_id,error.message,now()])?;
                tx.execute(
                    "UPDATE jobs SET last_error=?1 WHERE id=?2",
                    params![error.message, id],
                )?;
                tx.commit()?;
                Ok(false)
            }
        }
    }
    pub fn complete_ocr(&self, id: &str, token: &str, raw_text: String) -> Result<()> {
        if raw_text.len() > 500_000 {
            return Err(AppError::invalid("OCR output is too large."));
        }
        let settings = self.settings()?;
        if !settings.offline_ocr_enabled {
            return Err(AppError::new(
                "OcrDisabled",
                "Offline OCR is disabled. Enter this receipt manually.",
            ));
        }
        let result = LocalOcrExtractor.extract(&ExtractionInput {
            raw_text: &raw_text,
            images: &[],
            default_currency: &settings.default_currency,
        })?;
        self.complete_extraction(id, token, result, FieldSource::LocalOcr, Some(raw_text))
    }
    pub fn complete_extraction(
        &self,
        id: &str,
        token: &str,
        result: Extraction,
        source: FieldSource,
        raw_text: Option<String>,
    ) -> Result<()> {
        normalizer::validate(&result)?;
        let mut db = self.conn()?;
        let tx = db.transaction()?;
        let job = lease(&tx, id, token, Some("extract_receipt"))?;
        let mut e = expenses::get(&tx, &job.entity_id)?;
        if matches!(e.status, ExpenseStatus::Submitted | ExpenseStatus::Archived) {
            return Err(AppError::invalid("The expense is locked by its claim."));
        }
        macro_rules! merge {
            ($field:ident,$value:expr,$key:literal) => {
                if !e
                    .field_meta
                    .get($key)
                    .is_some_and(|m| m.source == FieldSource::Manual)
                {
                    if let Some(value) = $value {
                        e.$field = Some(value);
                        e.field_meta.insert(
                            $key.into(),
                            FieldMeta {
                                confidence: result.confidence.get($key).copied().unwrap_or(0.),
                                source: source.clone(),
                            },
                        );
                    }
                }
            };
        }
        merge!(merchant_name, result.merchant_name.clone(), "merchantName");
        merge!(occurred_at, result.date.clone(), "date");
        merge!(total_amount_minor, result.total_amount_minor, "total");
        merge!(tax_amount_minor, result.tax_amount_minor, "tax");
        merge!(currency, result.currency.clone(), "currency");
        if !e
            .field_meta
            .get("category")
            .is_some_and(|m| m.source == FieldSource::Manual)
        {
            if let Some(category) = result.suggested_category.clone() {
                e.category = category;
                e.field_meta.insert(
                    "category".into(),
                    FieldMeta {
                        confidence: result.confidence.get("category").copied().unwrap_or(0.),
                        source: source.clone(),
                    },
                );
            }
        }
        validate_values(&ExpenseEdit {
            id: e.id.clone(),
            version: e.version,
            occurred_at: e.occurred_at.clone(),
            merchant_name: e.merchant_name.clone(),
            total_amount_minor: e.total_amount_minor,
            tax_amount_minor: e.tax_amount_minor,
            currency: e.currency.clone(),
            category: e.category.clone(),
            description: e.description.clone(),
            mark_ready: false,
        })?;
        let confidence = ["merchantName", "date", "total", "currency"]
            .iter()
            .map(|k| e.field_meta.get(*k).map(|m| m.confidence).unwrap_or(0.))
            .fold(1., f64::min);
        let complete = e
            .merchant_name
            .as_ref()
            .is_some_and(|m| !m.trim().is_empty())
            && e.occurred_at.is_some()
            && e.total_amount_minor.is_some()
            && e.currency.is_some();
        if e.status != ExpenseStatus::Ready {
            e.status = if complete && confidence >= 0.9 {
                ExpenseStatus::Ready
            } else {
                ExpenseStatus::NeedsReview
            };
        }
        e.extraction_confidence = Some(confidence);
        e.updated_at = now();
        expenses::save(&tx, &e)?;
        if let Some(ref claim_id) = e.claim_id {
            tx.execute(
                "UPDATE expense_claims SET version=version+1,updated_at=?1 WHERE id=?2",
                params![e.updated_at, claim_id],
            )?;
        }
        tx.execute("INSERT INTO extraction_runs(id,expense_id,provider,raw_text,result_json,created_at) VALUES(?1,?2,?3,?4,?5,?6)",params![crate::domain::id(),e.id,match source{FieldSource::OnlineAi=>"online_ai",_=>"local_ocr"},raw_text,serde_json::to_string(&result)?,e.updated_at])?;
        tx.execute(
            "UPDATE jobs SET status='completed',lease_token=NULL,updated_at=?1 WHERE id=?2",
            params![e.updated_at, id],
        )?;
        repository::audit(
            &tx,
            "extraction.completed",
            &e.id,
            serde_json::json!({"source":source,"jobId":id}),
        )?;
        tx.commit()?;
        Ok(())
    }
    pub fn complete_thumbnail(&self, id: &str, token: &str, base64: &str) -> Result<()> {
        let bytes = STANDARD
            .decode(base64)
            .map_err(|_| AppError::invalid("Invalid thumbnail data."))?;
        if bytes.len() > 2 * 1024 * 1024 {
            return Err(AppError::invalid("Thumbnail is too large."));
        }
        // WebKit may return PNG even when canvas requests WebP. Decode bounded input,
        // then encode a canonical WebP in Rust so every platform stores the same format.
        let mut reader =
            image::ImageReader::new(std::io::Cursor::new(&bytes)).with_guessed_format()?;
        let mut limits = image::Limits::default();
        limits.max_image_width = Some(600);
        limits.max_image_height = Some(600);
        limits.max_alloc = Some(4 * 1024 * 1024);
        reader.limits(limits);
        let image = reader
            .decode()
            .map_err(|_| AppError::invalid("Invalid thumbnail image."))?;
        if image.width() > 600 || image.height() > 600 {
            return Err(AppError::invalid("Thumbnail dimensions are too large."));
        }
        let mut encoded = std::io::Cursor::new(Vec::new());
        image
            .write_to(&mut encoded, image::ImageFormat::WebP)
            .map_err(|_| AppError::invalid("Unable to encode thumbnail."))?;
        let bytes = encoded.into_inner();
        let mut db = self.conn()?;
        let tx = db.transaction()?;
        let job = lease(&tx, id, token, Some("generate_thumbnail"))?;
        let receipt = expenses::get(&tx, &job.entity_id)?
            .receipt_id
            .ok_or_else(AppError::not_found)?;
        let path = self
            .paths
            .resolve(&format!("receipts/{receipt}/preview.webp"))?;
        let temp = self.paths.resolve(&format!("cache/{id}.part"))?;
        {
            let mut file = fs::File::create(&temp)?;
            file.write_all(&bytes)?;
            file.sync_all()?;
        }
        if path.exists() {
            fs::remove_file(&path)?;
        }
        fs::rename(temp, path)?;
        tx.execute(
            "UPDATE jobs SET status='completed',lease_token=NULL,updated_at=?1 WHERE id=?2",
            params![now(), id],
        )?;
        tx.commit()?;
        Ok(())
    }
    pub fn export_snapshot(&self, id: &str, token: &str) -> Result<ExportSnapshot> {
        let db = self.conn()?;
        lease(&db, id, token, Some("generate_pdf"))?;
        let payload: String =
            db.query_row("SELECT payload FROM jobs WHERE id=?1", [id], |r| r.get(0))?;
        Ok(serde_json::from_str(&payload)?)
    }
    pub fn complete_pdf(&self, id: &str, token: &str, base64: &str) -> Result<String> {
        if base64.len() > 140 * 1024 * 1024 {
            return Err(AppError::new("PdfGenerationError","Report exceeds the 100 MB export limit. Split the claim or turn off receipt images."));
        }
        let bytes = STANDARD
            .decode(base64)
            .map_err(|_| AppError::new("PdfGenerationError", "Invalid PDF data."))?;
        if !bytes.starts_with(b"%PDF-") {
            return Err(AppError::new(
                "PdfGenerationError",
                "The report was not a valid PDF.",
            ));
        }
        let mut db = self.conn()?;
        let tx = db.transaction()?;
        let job = lease(&tx, id, token, Some("generate_pdf"))?;
        let relative = format!("exports/claims/{id}.pdf");
        let path = self.paths.resolve(&relative)?;
        let temp = self.paths.resolve(&format!("cache/{id}.part"))?;
        {
            let mut f = fs::File::create(&temp)?;
            f.write_all(&bytes)?;
            f.sync_all()?;
        }
        if path.exists() {
            fs::remove_file(&path)?;
        }
        fs::rename(&temp, &path)?;
        tx.execute(
            "UPDATE jobs SET status='completed',lease_token=NULL,updated_at=?1 WHERE id=?2",
            params![now(), id],
        )?;
        repository::audit(
            &tx,
            "claim.generated",
            &job.entity_id,
            serde_json::json!({"jobId":id,"relativePath":relative}),
        )?;
        let settings = Self::settings_in(&tx)?;
        tx.commit()?;
        drop(db);
        if let Some(folder) = settings.export_directory {
            let destination = std::path::Path::new(&folder).join(format!("Folio-claim-{id}.pdf"));
            let external = (|| -> std::io::Result<()> {
                let mut file = fs::OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(&destination)?;
                file.write_all(&bytes)?;
                file.sync_all()
            })();
            if external.is_err() {
                return Err(AppError::new("ExportCopyError",format!("The PDF is saved in Folio, but could not be copied to the export directory. Local copy: {}",path.display())));
            }
        }
        Ok(path.to_string_lossy().to_string())
    }
    pub fn export_path(&self, id: &str) -> Result<String> {
        let db = self.conn()?;
        let done:bool=db.query_row("SELECT EXISTS(SELECT 1 FROM jobs WHERE id=?1 AND job_type='generate_pdf' AND status='completed')",[id],|r|r.get(0))?;
        if !done {
            return Err(AppError::not_found());
        }
        uuid::Uuid::parse_str(id).map_err(|_| AppError::invalid("Invalid export ID."))?;
        Ok(self
            .paths
            .resolve(&format!("exports/claims/{id}.pdf"))?
            .to_string_lossy()
            .to_string())
    }
}
