use crate::error::{AppError, Result};
use std::{
    fs,
    path::{Component, Path, PathBuf},
};
#[derive(Debug, Clone)]
pub struct AppPaths {
    pub root: PathBuf,
}
impl AppPaths {
    pub fn new(root: PathBuf) -> Result<Self> {
        fs::create_dir_all(&root)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&root, fs::Permissions::from_mode(0o700))?;
        }
        for folder in [
            "database",
            "receipts",
            "exports/claims",
            "exports/csv",
            "cache",
            "logs",
        ] {
            fs::create_dir_all(root.join(folder))?;
        }
        Ok(Self {
            root: fs::canonicalize(root)?,
        })
    }
    pub fn resolve(&self, relative: &str) -> Result<PathBuf> {
        let path = Path::new(relative);
        if path.as_os_str().is_empty()
            || path
                .components()
                .any(|c| !matches!(c, Component::Normal(_)))
            || relative.contains('\\')
        {
            return Err(AppError::invalid("Invalid internal path."));
        }
        let target = self.root.join(path);
        let mut ancestor = target.as_path();
        while !ancestor.exists() {
            ancestor = ancestor
                .parent()
                .ok_or_else(|| AppError::invalid("Invalid path."))?;
        }
        if !fs::canonicalize(ancestor)?.starts_with(&self.root) {
            return Err(AppError::invalid(
                "Storage path escapes the application folder.",
            ));
        }
        Ok(target)
    }
    pub fn receipt_relative(id: &str, extension: &str) -> Result<String> {
        uuid::Uuid::parse_str(id).map_err(|_| AppError::invalid("Invalid receipt ID."))?;
        if !["png", "jpg", "pdf", "webp", "heic"].contains(&extension) {
            return Err(AppError::invalid("Invalid receipt format."));
        }
        Ok(format!("receipts/{id}/original.{extension}"))
    }
}
