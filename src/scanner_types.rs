//! Shared types for scanner, cache, and rules modules

use std::time::SystemTime;
use serde::{Serialize, Deserialize};

/// Directory fingerprint for incremental caching
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Fingerprint {
    pub mod_time_ns: i64,
}

/// File attributes relevant for classification rules
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Attrs {
    pub is_dir: bool,
    pub is_reparse: bool,
    pub is_hidden: bool,
    pub is_system: bool,
}

/// A single file record found by the scanner
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileRecord {
    pub path: String,
    pub size: i64,
    pub mod_time: SystemTime,
    pub attrs: Attrs,
}

/// Error encountered during scanning (non-fatal)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanError {
    pub path: String,
    pub error: String,
}

impl std::fmt::Display for ScanError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.path, self.error)
    }
}
impl std::error::Error for ScanError {}

/// Scanner configuration options
#[derive(Debug, Clone)]
pub struct Options {
    pub workers: usize,
    pub follow_links: bool,
    pub exclude: Vec<String>,
    pub exclude_pref: Vec<String>,
}

impl Default for Options {
    fn default() -> Self {
        Self { workers: 0, follow_links: false, exclude: Vec::new(), exclude_pref: Vec::new() }
    }
}

/// Cached directory entry
#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(dead_code)]
pub struct CacheEntry {
    pub fingerprint: Fingerprint,
    pub files: Vec<FileRecord>,
    pub dirs: Vec<String>,
}

/// Snapshot of scanning progress for UI/CLI display
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProgressSnapshot {
    pub files: i64,
    pub dirs: i64,
    pub bytes: i64,
    pub errors: i64,
    pub cached: i64,
    pub total: i64,
    pub percent: f64,
    /// True while the filesystem size is still being discovered and percent is an estimate.
    pub estimated: bool,
    pub current: String,
    pub elapsed_s: f64,
    pub rate_fps: f64,
    pub remain_s: f64,
    pub finished: bool,
    pub recent: Vec<String>,
}

/// Directory identity for cycle detection
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct DirId {
    pub volume_serial: u32,
    pub file_index: u64,
}

pub const FLUSH_BATCH: usize = 256;
pub const RECENT_CAP: usize = 50;
