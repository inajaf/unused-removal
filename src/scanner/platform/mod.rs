//! Cross-platform scanner abstraction

use std::sync::atomic::{AtomicBool, AtomicI64, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;
use crate::scanner_types::{FileRecord, ScanError, Options, ProgressSnapshot, RECENT_CAP};
use crate::cache::Cache;

#[cfg(windows)]
pub mod windows;
#[cfg(not(windows))]
pub mod unix;

#[cfg(windows)]
use self::windows::WindowsWalker;
#[cfg(not(windows))]
use self::unix::UnixWalker;

/// Platform-agnostic scanner trait
pub trait PlatformWalker: Send + Sync {
    fn walk(&self, root: &str) -> anyhow::Result<(Vec<FileRecord>, Vec<ScanError>)>;
    fn stop(&self);
}

/// Factory to create platform-specific walker
pub fn create_walker(opts: Options, progress: Progress, cache: Option<Arc<dyn Cache>>) -> Box<dyn PlatformWalker> {
    #[cfg(windows)]
    {
        Box::new(WindowsWalker::new(opts, progress, cache))
    }
    #[cfg(not(windows))]
    {
        Box::new(UnixWalker::new(opts, progress, cache))
    }
}

/// Progress tracker shared across platforms
#[derive(Clone)]
pub struct Progress {
    inner: Arc<ProgressInner>,
}

struct ProgressInner {
    files: AtomicU64,
    dirs: AtomicU64,
    bytes: AtomicU64,
    errors: AtomicU64,
    total: AtomicI64,
    finished: AtomicBool,
    started: Instant,
    cached: AtomicU64,
    recent: Arc<Mutex<Vec<String>>>,
}

impl ProgressInner {
    pub fn new() -> Self {
        Self {
            files: AtomicU64::new(0),
            dirs: AtomicU64::new(0),
            bytes: AtomicU64::new(0),
            errors: AtomicU64::new(0),
            total: AtomicI64::new(-1),
            finished: AtomicBool::new(false),
            started: Instant::now(),
            cached: AtomicU64::new(0),
            recent: Arc::new(Mutex::new(Vec::with_capacity(RECENT_CAP))),
        }
    }

    pub fn add_file(&self, size: i64) {
        self.files.fetch_add(1, Ordering::Relaxed);
        self.bytes.fetch_add(size as u64, Ordering::Relaxed);
    }

    pub fn add_dir(&self) {
        self.dirs.fetch_add(1, Ordering::Relaxed);
    }

    pub fn add_error(&self) {
        self.errors.fetch_add(1, Ordering::Relaxed);
    }

    pub fn add_cached(&self) {
        self.cached.fetch_add(1, Ordering::Relaxed);
    }

    pub fn set_total(&self, n: i64) {
        self.total.store(n, Ordering::Relaxed);
    }

    pub fn total(&self) -> i64 {
        self.total.load(Ordering::Relaxed)
    }

    pub fn finish(&self) {
        self.finished.store(true, Ordering::Relaxed);
    }

    pub fn add_recent_path(&self, path: String) {
        let mut recent = self.recent.lock().unwrap();
        recent.push(path);
        if recent.len() > RECENT_CAP {
            let drain_end = RECENT_CAP / 2;
            recent.drain(0..drain_end);
        }
    }

    pub fn snapshot(&self) -> ProgressSnapshot {
        let elapsed = self.started.elapsed().as_secs_f64();
        let files = self.files.load(Ordering::Relaxed);
        let total = self.total.load(Ordering::Relaxed);

        let mut snap = ProgressSnapshot {
            files: files as i64,
            dirs: self.dirs.load(Ordering::Relaxed) as i64,
            bytes: self.bytes.load(Ordering::Relaxed) as i64,
            errors: self.errors.load(Ordering::Relaxed) as i64,
            cached: self.cached.load(Ordering::Relaxed) as i64,
            total,
            percent: -1.0,
            current: String::new(),
            elapsed_s: elapsed,
            rate_fps: 0.0,
            remain_s: 0.0,
            finished: self.finished.load(Ordering::Relaxed),
            recent: self.recent.lock().unwrap().clone(),
        };

        if elapsed > 0.5 && files > 0 {
            snap.rate_fps = files as f64 / elapsed;
        }

        if total > 0 {
            if files > 0 {
                let pct = (files as f64 / total as f64) * 100.0;
                snap.percent = pct.min(100.0);
            } else {
                snap.percent = 0.0;
            }
            if snap.rate_fps > 0.0 && (files as i64) < total {
                snap.remain_s = (total - files as i64) as f64 / snap.rate_fps;
            }
        }

        if snap.finished {
            snap.percent = 100.0;
        }

        snap
    }
}

impl Progress {
    pub fn new() -> Self {
        Self { inner: Arc::new(ProgressInner::new()) }
    }

    pub fn add_file(&self, size: i64) { self.inner.add_file(size); }
    pub fn add_dir(&self) { self.inner.add_dir(); }
    pub fn add_error(&self) { self.inner.add_error(); }
    pub fn add_cached(&self) { self.inner.add_cached(); }
    pub fn set_total(&self, n: i64) { self.inner.set_total(n); }
    pub fn total(&self) -> i64 { self.inner.total() }
    pub fn finish(&self) { self.inner.finish(); }
    pub fn add_recent_path(&self, path: String) { self.inner.add_recent_path(path); }
    pub fn snapshot(&self) -> ProgressSnapshot { self.inner.snapshot() }
}

impl Default for Progress {
    fn default() -> Self { Self::new() }
}