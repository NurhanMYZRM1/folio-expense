use super::{heic, paths::AppPaths};
use crate::{
    domain::{id, now, receipt::ReceiptFile},
    error::{AppError, Result},
};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::Path,
};
pub const MAX_FILE_BYTES: u64 = 25 * 1024 * 1024;
/// Files the app derives from an original inside `receipts/<id>/`.
pub const DERIVED_FILES: [&str; 2] = ["preview.webp", heic::RENDITION];
/// Extensions an original may be stored under (`receipts/<id>/original.<ext>`).
pub const ORIGINAL_EXTENSIONS: [&str; 4] = ["png", "jpg", "pdf", "heic"];
/// A validated receipt in `cache/`, ready to publish.
pub struct Staged {
    pub receipt: ReceiptFile,
    pub temp: std::path::PathBuf,
    /// JPEG to show in place of an original WebViews can't draw (HEIC).
    pub rendition: Option<Vec<u8>>,
}
pub fn stage(paths: &AppPaths, source: &Path) -> Result<Staged> {
    let ext = source
        .extension()
        .and_then(|x| x.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    if !["png", "jpg", "jpeg", "pdf", "heic", "heif"].contains(&ext.as_str()) {
        return Err(AppError::new(
            "FileUnsupported",
            "Choose a PNG, JPG, JPEG, HEIC or PDF receipt.",
        ));
    }
    let mut input = File::open(source)?;
    let meta = input.metadata()?;
    if !meta.is_file() {
        return Err(AppError::new("FileUnsupported", "Select a regular file."));
    }
    if meta.len() == 0 || meta.len() > MAX_FILE_BYTES {
        return Err(AppError::new(
            "FileTooLarge",
            "Receipts must be between 1 byte and 25 MB.",
        ));
    }
    let receipt_id = id();
    let temp = paths.resolve(&format!("cache/{receipt_id}.part"))?;
    let result = (|| -> Result<(ReceiptFile, Option<Vec<u8>>)> {
        let mut opts = OpenOptions::new();
        opts.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            opts.mode(0o600);
        }
        let mut output = opts.open(&temp)?;
        let mut buffer = [0u8; 65536];
        let mut hash = Sha256::new();
        let mut size = 0u64;
        let mut header = Vec::new();
        loop {
            let n = input.read(&mut buffer)?;
            if n == 0 {
                break;
            }
            size += n as u64;
            if size > MAX_FILE_BYTES {
                return Err(AppError::new(
                    "FileTooLarge",
                    "Receipt exceeds the 25 MB limit.",
                ));
            }
            if header.is_empty() {
                header.extend_from_slice(&buffer[..n.min(16)]);
            }
            output.write_all(&buffer[..n])?;
            hash.update(&buffer[..n]);
        }
        output.sync_all()?;
        let (mime, extension) = if header.starts_with(b"\x89PNG\r\n\x1a\n") && ext == "png" {
            ("image/png", "png")
        } else if header.starts_with(&[0xff, 0xd8, 0xff]) && ["jpg", "jpeg"].contains(&ext.as_str())
        {
            ("image/jpeg", "jpg")
        } else if header.starts_with(b"%PDF-") && ext == "pdf" {
            ("application/pdf", "pdf")
        } else if heic::is_heif(&header) && ["heic", "heif"].contains(&ext.as_str()) {
            (heic::MIME, "heic")
        } else {
            return Err(AppError::new(
                "FileUnsupported",
                "The file contents do not match a supported receipt format.",
            ));
        };
        let mut rendition = None;
        if extension == "heic" {
            // Decoding fully here both validates the file and builds the rendition.
            rendition = Some(heic::rendition(&temp)?);
        } else if extension != "pdf" {
            let dims = image::ImageReader::open(&temp)?
                .with_guessed_format()?
                .into_dimensions()
                .map_err(|_| AppError::new("FileUnsupported", "This image cannot be decoded."))?;
            if dims.0 == 0 || dims.1 == 0 || u64::from(dims.0) * u64::from(dims.1) > 40_000_000 {
                return Err(AppError::new(
                    "FileTooLarge",
                    "Receipt images must be under 40 megapixels.",
                ));
            }
        }
        let receipt = ReceiptFile {
            id: receipt_id.clone(),
            sha256: format!("{:x}", hash.finalize()),
            original_filename: source
                .file_name()
                .map(|n| n.to_string_lossy().chars().take(255).collect())
                .unwrap_or_else(|| "Receipt".into()),
            mime_type: mime.into(),
            relative_path: AppPaths::receipt_relative(&receipt_id, extension)?,
            size_bytes: size as i64,
            created_at: now(),
        };
        Ok((receipt, rendition))
    })();
    match result {
        Ok((receipt, rendition)) => Ok(Staged {
            receipt,
            temp,
            rendition,
        }),
        Err(e) => {
            let _ = fs::remove_file(&temp);
            Err(e)
        }
    }
}
pub fn publish(paths: &AppPaths, staged: &Staged) -> Result<()> {
    let target = paths.resolve(&staged.receipt.relative_path)?;
    let parent = target
        .parent()
        .ok_or_else(|| AppError::invalid("Invalid receipt path."))?;
    fs::create_dir_all(parent)?;
    if let Some(ref jpeg) = staged.rendition {
        write_rendition(paths, &staged.receipt.id, jpeg)?;
    }
    fs::rename(&staged.temp, &target)?;
    #[cfg(unix)]
    {
        File::open(parent)?.sync_all()?;
        File::open(paths.root.join("receipts"))?.sync_all()?;
    }
    Ok(())
}
/// Atomically stores the display rendition for receipt `id`.
pub fn write_rendition(paths: &AppPaths, id: &str, jpeg: &[u8]) -> Result<()> {
    let target = paths.resolve(&format!("receipts/{id}/{}", heic::RENDITION))?;
    let temp = paths.resolve(&format!("cache/{}.part", crate::domain::id()))?;
    let result = (|| -> Result<()> {
        let mut opts = OpenOptions::new();
        opts.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            opts.mode(0o600);
        }
        let mut file = opts.open(&temp)?;
        file.write_all(jpeg)?;
        file.sync_all()?;
        fs::rename(&temp, &target)?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temp);
    }
    result
}
