//! File deletion operations: Recycle Bin and Hard Delete using Windows Shell API

use std::path::PathBuf;
use std::ffi::OsStr;
use std::os::windows::ffi::OsStrExt;
use windows::Win32::UI::Shell::{SHFileOperationW, FO_DELETE, FOF_ALLOWUNDO, FOF_NOCONFIRMATION, FOF_NOERRORUI, FOF_SILENT};
use windows::Win32::Foundation::{HWND, BOOL};
use windows::core::PCWSTR;
use anyhow::Result;

const FOF_WANTNUKE: u16 = 0x0001; // Not exposed in windows crate

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

/// Move files/directories to Recycle Bin
pub fn recycle_bin(paths: &[String]) -> Result<DeleteResult> {
    if paths.is_empty() {
        return Ok(DeleteResult { deleted: Vec::new(), failed: Vec::new(), total_bytes: 0 });
    }
    do_sh_file_op(paths, FOF_ALLOWUNDO | FOF_NOCONFIRMATION | FOF_NOERRORUI | FOF_SILENT)
}

/// Permanently delete files/directories (bypass Recycle Bin)
pub fn hard_delete(paths: &[String]) -> Result<DeleteResult> {
    if paths.is_empty() {
        return Ok(DeleteResult { deleted: Vec::new(), failed: Vec::new(), total_bytes: 0 });
    }
    
    // Try SHFileOperation with FOF_WANTNUKE first
    match do_sh_file_op(paths, FOF_WANTNUKE | FOF_NOCONFIRMATION | FOF_NOERRORUI | FOF_SILENT) {
        Ok(result) => Ok(result),
        Err(_) => hard_delete_fallback(paths),
    }
}

fn do_sh_file_op(paths: &[String], flags: u16) -> Result<DeleteResult> {
    // Build double-null-terminated string list in UTF-16
    let mut from = Vec::new();
    let mut total_bytes = 0u64;
    let mut valid_paths = Vec::new();

    for p in paths {
        let abs = std::path::absolute(p)?;
        // Check existence
        if !abs.exists() { continue; }
        // Get file size for statistics (files only, not directories)
        if let Ok(metadata) = std::fs::metadata(&abs) {
            if metadata.is_file() { total_bytes += metadata.len(); }
        }
        // Convert to wide string
        let abs_str = abs.to_string_lossy();
        let wide: Vec<u16> = OsStr::new(&abs_str).encode_wide().chain(Some(0)).collect();
        from.extend(wide);
        valid_paths.push(abs_str.to_string());
    }

    if from.is_empty() {
        return Ok(DeleteResult { deleted: Vec::new(), failed: Vec::new(), total_bytes: 0 });
    }

    // Add final null terminator
    from.push(0);

    let mut fileop = windows::Win32::UI::Shell::SHFILEOPSTRUCTW {
        hwnd: HWND(std::ptr::null_mut()),
        wFunc: FO_DELETE,
        pFrom: PCWSTR(from.as_ptr()),
        pTo: PCWSTR::null(),
        fFlags: flags,
        fAnyOperationsAborted: BOOL(0),
        hNameMappings: std::ptr::null_mut(),
        lpszProgressTitle: PCWSTR::null(),
    };

    let result = unsafe { SHFileOperationW(&mut fileop) };
    
    if result != 0 {
        return Err(anyhow::anyhow!("SHFileOperation failed with code {}", result));
    }

    Ok(DeleteResult { deleted: valid_paths, failed: Vec::new(), total_bytes })
}

/// Fallback: manual deletion using std::fs::remove_dir_all / remove_file
fn hard_delete_fallback(paths: &[String]) -> Result<DeleteResult> {
    use std::sync::{Arc, Mutex};

    let deleted = Arc::new(Mutex::new(Vec::new()));
    let failed = Arc::new(Mutex::new(Vec::new()));
    let total_bytes = Arc::new(Mutex::new(0u64));

    std::thread::scope(|s| {
        for p in paths {
            let abs = match std::path::absolute(p) {
                Ok(a) => a,
                Err(e) => {
                    failed.lock().unwrap().push(DeleteError { path: p.clone(), error: e.to_string() });
                    continue;
                }
            };

            let deleted = deleted.clone();
            let failed = failed.clone();
            let total_bytes = total_bytes.clone();
            let p_clone = p.clone();

            s.spawn(move || {
                if let Ok(metadata) = std::fs::metadata(&abs) {
                    if metadata.is_file() {
                        *total_bytes.lock().unwrap() += metadata.len();
                    }
                }

                let result = if abs.is_dir() {
                    std::fs::remove_dir_all(&abs)
                } else {
                    std::fs::remove_file(&abs)
                };

                match result {
                    Ok(_) => deleted.lock().unwrap().push(p_clone),
                    Err(e) => failed.lock().unwrap().push(DeleteError { path: p_clone, error: e.to_string() }),
                }
            });
        }
    });

    let total_bytes_val = *total_bytes.lock().unwrap();
    Ok(DeleteResult {
        deleted: Arc::try_unwrap(deleted).unwrap().into_inner().unwrap(),
        failed: Arc::try_unwrap(failed).unwrap().into_inner().unwrap(),
        total_bytes: total_bytes_val,
    })
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
}