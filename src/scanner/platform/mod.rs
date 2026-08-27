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
    percent_override: AtomicI64,
    segment_start_files: AtomicU64,
    segment_start_dirs: AtomicU64,
    segment_start_percent: AtomicI64,
    segment_end_percent: AtomicI64,
    discovery_percent: AtomicI64,
    started: Instant,
    cached: AtomicU64,
    current: Mutex<String>,
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
            percent_override: AtomicI64::new(-1),
            segment_start_files: AtomicU64::new(0),
            segment_start_dirs: AtomicU64::new(0),
            segment_start_percent: AtomicI64::new(0),
            segment_end_percent: AtomicI64::new(9_000),
            discovery_percent: AtomicI64::new(0),
            started: Instant::now(),
            cached: AtomicU64::new(0),
            current: Mutex::new(String::new()),
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

    pub fn begin_segment(&self, index: usize, count: usize) {
        let count = count.max(1) as i64;
        let index = index.min(count as usize - 1) as i64;
        self.segment_start_files
            .store(self.files.load(Ordering::Relaxed), Ordering::Relaxed);
        self.segment_start_dirs
            .store(self.dirs.load(Ordering::Relaxed), Ordering::Relaxed);
        let start = index * 9_000 / count;
        self.segment_start_percent.store(start, Ordering::Relaxed);
        self.segment_end_percent
            .store((index + 1) * 9_000 / count, Ordering::Relaxed);
        self.discovery_percent.store(start, Ordering::Relaxed);
        self.percent_override.store(-1, Ordering::Relaxed);
        self.total.store(-1, Ordering::Relaxed);
    }

    pub fn set_phase(&self, current: impl Into<String>, percent: f64) {
        *self.current.lock().unwrap() = current.into();
        self.set_percent(percent);
    }

    pub fn set_percent(&self, percent: f64) {
        self.percent_override
            .store((percent.clamp(0.0, 99.0) * 100.0).round() as i64, Ordering::Relaxed);
    }

    pub fn set_current(&self, current: impl Into<String>) {
        *self.current.lock().unwrap() = current.into();
    }

    pub fn update_discovery(&self, pending_dirs: u64) {
        let completed_dirs = self
            .dirs
            .load(Ordering::Relaxed)
            .saturating_sub(self.segment_start_dirs.load(Ordering::Relaxed));
        let discovered = completed_dirs.saturating_add(pending_dirs);
        if discovered == 0 {
            return;
        }
        let start = self.segment_start_percent.load(Ordering::Relaxed);
        let end = self.segment_end_percent.load(Ordering::Relaxed);
        let fraction = completed_dirs as f64 / discovered as f64;
        let estimate = start + ((end - start) as f64 * fraction).round() as i64;
        self.discovery_percent.fetch_max(estimate.min(end - 1), Ordering::Relaxed);
    }

    pub fn total(&self) -> i64 {
        self.total.load(Ordering::Relaxed)
    }

    pub fn finish(&self) {
        self.percent_override.store(10_000, Ordering::Relaxed);
        *self.current.lock().unwrap() = "Завершено".to_string();
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
            estimated: false,
            current: self.current.lock().unwrap().clone(),
            elapsed_s: elapsed,
            rate_fps: 0.0,
            remain_s: 0.0,
            finished: self.finished.load(Ordering::Relaxed),
            recent: self.recent.lock().unwrap().clone(),
        };

        if elapsed > 0.5 && files > 0 {
            snap.rate_fps = files as f64 / elapsed;
        }

        let percent_override = self.percent_override.load(Ordering::Relaxed);
        if percent_override >= 0 {
            snap.percent = percent_override as f64 / 100.0;
        } else if total > 0 {
            let segment_start_files = self.segment_start_files.load(Ordering::Relaxed);
            let segment_files = files.saturating_sub(segment_start_files);
            let start = self.segment_start_percent.load(Ordering::Relaxed) as f64 / 100.0;
            let end = self.segment_end_percent.load(Ordering::Relaxed) as f64 / 100.0;
            let local_fraction = (segment_files as f64 / total as f64).clamp(0.0, 1.0);
            snap.percent = start + (end - start) * local_fraction;
            if snap.rate_fps > 0.0 && (segment_files as i64) < total {
                snap.remain_s = (total - segment_files as i64) as f64 / snap.rate_fps;
            }
        } else {
            snap.percent = self.discovery_percent.load(Ordering::Relaxed) as f64 / 100.0;
            snap.estimated = true;
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
    pub fn begin_segment(&self, index: usize, count: usize) { self.inner.begin_segment(index, count); }
    pub fn set_phase(&self, current: impl Into<String>, percent: f64) { self.inner.set_phase(current, percent); }
    pub fn set_percent(&self, percent: f64) { self.inner.set_percent(percent); }
    pub fn set_current(&self, current: impl Into<String>) { self.inner.set_current(current); }
    pub fn update_discovery(&self, pending_dirs: u64) { self.inner.update_discovery(pending_dirs); }
    pub fn total(&self) -> i64 { self.inner.total() }
    pub fn finish(&self) { self.inner.finish(); }
    pub fn add_recent_path(&self, path: String) { self.inner.add_recent_path(path); }
    pub fn snapshot(&self) -> ProgressSnapshot { self.inner.snapshot() }
}

impl Default for Progress {
    fn default() -> Self { Self::new() }
}

#[cfg(test)]
mod tests {
    use super::Progress;

    #[test]
    fn progress_reaches_one_hundred_only_after_finish() {
        let progress = Progress::new();
        progress.begin_segment(0, 2);
        progress.set_total(10);
        for _ in 0..10 {
            progress.add_file(1);
        }
        assert_eq!(progress.snapshot().percent, 45.0);

        progress.begin_segment(1, 2);
        progress.set_total(4);
        for _ in 0..4 {
            progress.add_file(1);
        }
        let scanning = progress.snapshot();
        assert_eq!(scanning.percent, 90.0);
        assert!(!scanning.finished);

        progress.set_phase("Подготовка результатов…", 99.0);
        assert_eq!(progress.snapshot().percent, 99.0);

        progress.finish();
        let finished = progress.snapshot();
        assert_eq!(finished.percent, 100.0);
        assert!(finished.finished);
    }
}
