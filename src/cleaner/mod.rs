//! Cross-platform file deletion operations
//! 
//! Uses Windows Recycle Bin API on Windows and freedesktop trash spec on Linux/macOS

use std::fs;
use anyhow::Result;
use trash;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DeleteResult {
    pub deleted: Vec<String>,
    pub failed: Vec<DeleteError>,
    pub total_bytes: u64,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DeleteError {
    pub path: String,
    pub error: String,
}

impl std::fmt::Display for DeleteError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.path, self.error)
    }
}

impl std::error::Error for DeleteError {}

/// Move files/directories to platform trash/recycle bin
pub fn recycle_bin(paths: &[String]) -> Result<DeleteResult> {
    if paths.is_empty() {
        return Ok(DeleteResult { deleted: Vec::new(), failed: Vec::new(), total_bytes: 0 });
    }

    let mut deleted = Vec::new();
    let mut failed = Vec::new();
    let mut total_bytes = 0u64;

    for p in paths {
        let abs = match std::path::absolute(p) {
            Ok(a) => a,
            Err(e) => {
                failed.push(DeleteError { path: p.clone(), error: e.to_string() });
                continue;
            }
        };

        if !abs.exists() {
            failed.push(DeleteError { path: p.clone(), error: "Path does not exist".to_string() });
            continue;
        }

        // Get file size for statistics (files only, not directories)
        if let Ok(metadata) = fs::metadata(&abs) {
            if metadata.is_file() { total_bytes += metadata.len(); }
        }

        // Use cross-platform trash crate
        match trash::delete(&abs) {
            Ok(_) => deleted.push(p.clone()),
            Err(e) => failed.push(DeleteError { path: p.clone(), error: e.to_string() }),
        }
    }

    Ok(DeleteResult { deleted, failed, total_bytes })
}

/// Permanently delete files/directories (bypass trash/recycle bin)
pub fn hard_delete(paths: &[String]) -> Result<DeleteResult> {
    if paths.is_empty() {
        return Ok(DeleteResult { deleted: Vec::new(), failed: Vec::new(), total_bytes: 0 });
    }

    let mut deleted = Vec::new();
    let mut failed = Vec::new();
    let mut total_bytes = 0u64;

    for p in paths {
        let abs = match std::path::absolute(p) {
            Ok(a) => a,
            Err(e) => {
                failed.push(DeleteError { path: p.clone(), error: e.to_string() });
                continue;
            }
        };

        if !abs.exists() {
            failed.push(DeleteError { path: p.clone(), error: "Path does not exist".to_string() });
            continue;
        }

        // Get file size for statistics (files only, not directories)
        if let Ok(metadata) = fs::metadata(&abs) {
            if metadata.is_file() { total_bytes += metadata.len(); }
        }

        // Direct deletion
        let result = if abs.is_dir() {
            fs::remove_dir_all(&abs)
        } else {
            fs::remove_file(&abs)
        };

        match result {
            Ok(_) => deleted.push(p.clone()),
            Err(e) => failed.push(DeleteError { path: p.clone(), error: e.to_string() }),
        }
    }

    Ok(DeleteResult { deleted, failed, total_bytes })
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;
    use std::fs::File;
    use std::io::Write;

    #[test]
    fn test_recycle_bin_creates_file() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("test.txt");
        File::create(&file_path).unwrap().write_all(b"test").unwrap();

        let paths = vec![file_path.to_string_lossy().to_string()];
        let _ = recycle_bin(&paths);
    }

    #[test]
    fn test_hard_delete() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("test.txt");
        File::create(&file_path).unwrap().write_all(b"test").unwrap();

        let paths = vec![file_path.to_string_lossy().to_string()];
        let result = hard_delete(&paths).unwrap();
        assert_eq!(result.deleted.len(), 1);
    }
}