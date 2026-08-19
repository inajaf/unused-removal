//! Parallel file system scanner for Windows using Win32 API.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicI64, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, Condvar};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use windows::Win32::Foundation::{FILETIME, INVALID_HANDLE_VALUE, CloseHandle};
use windows::Win32::Storage::FileSystem::{
    FindFirstFileExW, FindNextFileW, FindClose, FIND_FIRST_EX_LARGE_FETCH, FINDEX_INFO_LEVELS,
    FILE_ATTRIBUTE_DIRECTORY, FILE_ATTRIBUTE_REPARSE_POINT, FILE_ATTRIBUTE_HIDDEN, FILE_ATTRIBUTE_SYSTEM,
    WIN32_FIND_DATAW, CreateFileW, GetFileInformationByHandle, FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT,
    FILE_SHARE_READ, FILE_SHARE_WRITE, FILE_SHARE_DELETE, OPEN_EXISTING,
    BY_HANDLE_FILE_INFORMATION,
};
use windows::Win32::Foundation::GENERIC_READ;
use windows::core::PCWSTR;

use crate::cache::Cache;
use crate::scanner_types::{CacheEntry, Fingerprint, Attrs, FileRecord, ScanError, Options, ProgressSnapshot, DirId, FLUSH_BATCH, RECENT_CAP};

/// Re-export shared types from scanner_types
pub use crate::scanner_types::*;

/// RAII guard to ensure total_tasks is decremented when a directory task completes
struct TaskGuard<'a> {
    walker: &'a Walker,
    active: bool,
}

impl<'a> TaskGuard<'a> {
    fn new(walker: &'a Walker) -> Self {
        Self { walker, active: true }
    }
    
    fn disarm(mut self) {
        self.active = false;
    }
}

impl<'a> Drop for TaskGuard<'a> {
    fn drop(&mut self) {
        if self.active {
            let prev = self.walker.total_tasks.load(Ordering::Relaxed);
            if prev > 0 {
                self.walker.total_tasks.fetch_sub(1, Ordering::Relaxed);
                if self.walker.total_tasks.load(Ordering::Relaxed) == 0 {
                    self.walker.queue_not_empty.notify_all();
                }
            }
        }
    }
}

/// Thread-safe progress tracker
pub struct ProgressInner {
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

/// Wrapper for Progress to allow sharing
#[derive(Clone)]
pub struct Progress {
    inner: Arc<ProgressInner>,
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

/// Main parallel directory walker
pub struct Walker {
    opts: Options,
    progress: Progress,
    cache: Option<Arc<dyn Cache>>,
    queue: Arc<Mutex<Vec<String>>>,
    queue_not_empty: Arc<Condvar>,
    workers_done: Arc<Mutex<usize>>,
    total_tasks: AtomicU64,
    recs: Arc<Mutex<Vec<FileRecord>>>,
    errs: Arc<Mutex<Vec<ScanError>>>,
    exclude_lower: Vec<String>,
    pref_lower: Vec<String>,
    seen_dirs: Arc<Mutex<std::collections::HashSet<DirId>>>,
    stopped: AtomicBool,
}

impl Walker {
    pub fn new(opts: Options, progress: Progress, cache: Option<Arc<dyn Cache>>) -> Self {
        let workers = if opts.workers == 0 { num_cpus::get() } else { opts.workers };

        let mut exclude_lower = Vec::with_capacity(opts.exclude.len());
        for e in &opts.exclude { exclude_lower.push(e.to_lowercase()); }
        let mut pref_lower = Vec::with_capacity(opts.exclude_pref.len());
        for p in &opts.exclude_pref { pref_lower.push(p.trim_end_matches(['\\', '/']).to_lowercase()); }

        Self {
            opts: Options { workers, ..opts },
            progress,
            cache,
            queue: Arc::new(Mutex::new(Vec::with_capacity(4096))),
            queue_not_empty: Arc::new(Condvar::new()),
            workers_done: Arc::new(Mutex::new(0)),
            total_tasks: AtomicU64::new(0),
            recs: Arc::new(Mutex::new(Vec::new())),
            errs: Arc::new(Mutex::new(Vec::new())),
            exclude_lower,
            pref_lower,
            seen_dirs: Arc::new(Mutex::new(std::collections::HashSet::new())),
            stopped: AtomicBool::new(false),
        }
    }

    pub fn progress(&self) -> Progress { self.progress.clone() }

    pub fn walk(&self, root: &str) -> anyhow::Result<(Vec<FileRecord>, Vec<ScanError>)> {
        let root_path = std::fs::canonicalize(root)?;
        let root_str = root_path.to_string_lossy().to_string();

        if let Some(cache) = &self.cache {
            if let Some(total) = cache.load_total() {
                self.progress.set_total(total as i64);
            }
        }

        let mut handles = Vec::with_capacity(self.opts.workers);
        for _ in 0..self.opts.workers {
            let walker = self.clone();
            handles.push(thread::spawn(move || walker.worker_loop()));
        }

        {
            let mut queue = self.queue.lock().unwrap();
            queue.push(root_str);
            self.total_tasks.fetch_add(1, Ordering::Relaxed);
        }
        self.queue_not_empty.notify_one();

        for handle in handles { handle.join().unwrap(); }

        self.progress.finish();

        let recs = self.recs.lock().unwrap().clone();
        let errs = self.errs.lock().unwrap().clone();

        if let Some(cache) = &self.cache {
            let _ = cache.save_total(recs.len() as i64);
        }

        Ok((recs, errs))
    }

    fn worker_loop(&self) {
        loop {
            let dir = {
                let mut queue = self.queue.lock().unwrap();
                loop {
                    if let Some(dir) = queue.pop() { break Some(dir); }
                    // Break if no more tasks pending (queue empty and total_tasks == 0)
                    if self.total_tasks.load(Ordering::Relaxed) == 0 { break None; }
                    queue = self.queue_not_empty.wait(queue).unwrap();
                }
            };

            let Some(dir) = dir else {
                let mut done = self.workers_done.lock().unwrap();
                *done += 1;
                if *done == self.opts.workers { self.queue_not_empty.notify_all(); }
                break;
            };

            self.process_dir(&dir);
        }
    }

    fn process_dir(&self, dir: &str) {
        // Decrement task counter at the start to ensure it's called even on early return
        let _task_guard = TaskGuard::new(self);
        self.progress.add_dir();

        let (fingerprint, entries) = match read_dir_entries(dir) {
            Ok((fp, entries)) => (fp, entries),
            Err(e) => { self.add_error(dir.to_string(), e.to_string()); return; }
        };

        if let Some(cache) = &self.cache {
            let cache_key = cache_key(dir);
            if let Some(entry) = cache.lookup(&cache_key, &fingerprint) {
                self.progress.add_cached();
                self.add_cached_entries(&entry);
                for sub in &entry.dirs {
                    let child = format!("{}\\{}", dir.trim_end_matches('\\'), sub);
                    if !self.excluded(&child) { self.push_dir(&child); }
                }
                return;
            }
        }

        let mut files = Vec::new();
        let mut subdirs = Vec::new();

        for entry in entries {
            if entry.is_dir {
                if entry.is_reparse && !self.opts.follow_links { continue; }
                let child = format!("{}\\{}", dir.trim_end_matches('\\'), entry.name);
                if self.excluded(&child) { continue; }
                if entry.is_reparse && !self.check_follow_cycle(&child) { continue; }
                subdirs.push(entry.name);
                self.push_dir(&child);
            } else {
                let path = format!("{}\\{}", dir.trim_end_matches('\\'), entry.name);
                let record = FileRecord { path, size: entry.size, mod_time: entry.mod_time, attrs: entry.attrs };
                files.push(record.clone());
                self.add_record(record);
            }
        }

        if let Some(cache) = &self.cache {
            let cache_key = cache_key(dir);
            let _ = cache.save(&cache_key, CacheEntry { fingerprint, files, dirs: subdirs });
        }
    }

    fn add_cached_entries(&self, entry: &CacheEntry) {
        let mut recs = self.recs.lock().unwrap();
        for f in &entry.files {
            recs.push(f.clone());
            self.progress.add_file(f.size);
            self.progress.add_recent_path(f.path.clone());
        }
    }

    fn add_record(&self, record: FileRecord) {
        self.progress.add_file(record.size);
        self.progress.add_recent_path(record.path.clone());
        let mut local = Vec::new();
        local.push(record);
        if local.len() >= FLUSH_BATCH { self.flush_records(local); }
    }

    fn flush_records(&self, mut local: Vec<FileRecord>) {
        if local.is_empty() { return; }
        let mut recs = self.recs.lock().unwrap();
        recs.append(&mut local);
    }

    fn add_error(&self, path: String, error: String) {
        self.progress.add_error();
        self.errs.lock().unwrap().push(ScanError { path, error });
    }

    fn push_dir(&self, dir: &str) {
        self.total_tasks.fetch_add(1, Ordering::Relaxed);
        { let mut queue = self.queue.lock().unwrap(); queue.push(dir.to_string()); }
        self.queue_not_empty.notify_one();
    }

    fn excluded(&self, dir: &str) -> bool {
        if self.pref_lower.is_empty() && self.exclude_lower.is_empty() { return false; }
        let dir_lower = dir.to_lowercase();
        let dir_trimmed = dir_lower.trim_end_matches(['\\', '/']);
        for p in &self.pref_lower {
            if dir_trimmed == p || dir_trimmed.starts_with(&format!("{}\\", p)) || dir_trimmed.starts_with(&format!("{}/", p)) { return true; }
        }
        let base = Path::new(dir).file_name().and_then(|s| s.to_str()).unwrap_or("");
        let base_lower = base.to_lowercase();
        for e in &self.exclude_lower { if base_lower == *e { return true; } }
        false
    }

    fn check_follow_cycle(&self, path: &str) -> bool {
        if let Ok(id) = dir_identity(path) {
            let mut seen = self.seen_dirs.lock().unwrap();
            if seen.contains(&id) { return false; }
            seen.insert(id); true
        } else { true }
    }

    pub fn stop(&self) {
        self.stopped.store(true, Ordering::Relaxed);
        self.queue_not_empty.notify_all();
    }
}

impl Clone for Walker {
    fn clone(&self) -> Self {
        Self {
            opts: self.opts.clone(),
            progress: self.progress.clone(),
            cache: self.cache.clone(),
            queue: self.queue.clone(),
            queue_not_empty: self.queue_not_empty.clone(),
            workers_done: self.workers_done.clone(),
            total_tasks: AtomicU64::new(self.total_tasks.load(Ordering::Relaxed)),
            recs: self.recs.clone(),
            errs: self.errs.clone(),
            exclude_lower: self.exclude_lower.clone(),
            pref_lower: self.pref_lower.clone(),
            seen_dirs: self.seen_dirs.clone(),
            stopped: AtomicBool::new(self.stopped.load(Ordering::Relaxed)),
        }
    }
}

/// Win32 directory entry
#[derive(Debug)]
struct WinEntry {
    name: String,
    size: i64,
    mod_time: SystemTime,
    is_dir: bool,
    is_reparse: bool,
    attrs: Attrs,
}

fn read_dir_entries(dir: &str) -> anyhow::Result<(Fingerprint, Vec<WinEntry>)> {
    let pattern = build_search_pattern(dir);
    let mut find_data: WIN32_FIND_DATAW = unsafe { std::mem::zeroed() };
    let handle = unsafe {
        FindFirstFileExW(
            PCWSTR(pattern.as_ptr()),
            FINDEX_INFO_LEVELS(1), // FindExInfoBasic
            &mut find_data as *mut _ as *mut _,
            windows::Win32::Storage::FileSystem::FINDEX_SEARCH_OPS(0),
            None,
            FIND_FIRST_EX_LARGE_FETCH,
        )?
    };

    let fingerprint = Fingerprint { mod_time_ns: filetime_to_unix_ns(find_data.ftLastWriteTime) };
    let mut entries = Vec::new();

    loop {
        let name = wide_to_string(&find_data.cFileName);
        if name != "." && name != ".." {
            let attrs = win32_attrs_to_attrs(find_data.dwFileAttributes);
            entries.push(WinEntry {
                name,
                size: ((find_data.nFileSizeHigh as i64) << 32) | (find_data.nFileSizeLow as i64),
                mod_time: filetime_to_system_time(find_data.ftLastWriteTime),
                is_dir: attrs.is_dir, is_reparse: attrs.is_reparse, attrs,
            });
        }
        let result = unsafe { FindNextFileW(handle, &mut find_data) };
        if result.is_err() {
            let err = result.unwrap_err();
            if err.code() == windows::Win32::Foundation::ERROR_NO_MORE_FILES.to_hresult() { break; }
            return Err(err.into());
        }
    }
    unsafe { FindClose(handle) }?;
    Ok((fingerprint, entries))
}

fn build_search_pattern(dir: &str) -> Vec<u16> {
    let mut path = PathBuf::from(dir);
    if !path.ends_with("\\") { path.push("*"); } else { path.push("*"); }
    let path_str = path.to_string_lossy().to_string();
    let final_path = if path_str.len() > 248 && !path_str.starts_with(r"\\?\") { format!(r"\\?\{}", path_str) } else { path_str };
    string_to_wide(&final_path)
}

fn string_to_wide(s: &str) -> Vec<u16> {
    use std::os::windows::ffi::OsStrExt;
    std::ffi::OsStr::new(s).encode_wide().chain(Some(0)).collect()
}

fn wide_to_string(slice: &[u16]) -> String {
    let len = slice.iter().position(|&c| c == 0).unwrap_or(slice.len());
    String::from_utf16_lossy(&slice[..len])
}

fn win32_attrs_to_attrs(attrs: u32) -> Attrs {
    Attrs { is_dir: attrs & FILE_ATTRIBUTE_DIRECTORY.0 != 0, is_reparse: attrs & FILE_ATTRIBUTE_REPARSE_POINT.0 != 0, is_hidden: attrs & FILE_ATTRIBUTE_HIDDEN.0 != 0, is_system: attrs & FILE_ATTRIBUTE_SYSTEM.0 != 0 }
}

fn filetime_to_system_time(ft: FILETIME) -> SystemTime {
    let ticks = ((ft.dwHighDateTime as u64) << 32) | (ft.dwLowDateTime as u64);
    const TICKS_PER_SEC: u64 = 10_000_000;
    const UNIX_EPOCH_OFFSET: u64 = 116_444_736_000_000_000;
    let unix_ticks = ticks.saturating_sub(UNIX_EPOCH_OFFSET);
    let secs = unix_ticks / TICKS_PER_SEC;
    let nanos = ((unix_ticks % TICKS_PER_SEC) * 100) as u32;
    UNIX_EPOCH + Duration::new(secs, nanos)
}

fn filetime_to_unix_ns(ft: FILETIME) -> i64 {
    let ticks = ((ft.dwHighDateTime as u64) << 32) | (ft.dwLowDateTime as u64);
    const TICKS_PER_SEC: u64 = 10_000_000;
    const UNIX_EPOCH_OFFSET: u64 = 116_444_736_000_000_000;
    let unix_ticks = ticks.saturating_sub(UNIX_EPOCH_OFFSET);
    let secs = unix_ticks / TICKS_PER_SEC;
    let nanos = (unix_ticks % TICKS_PER_SEC) * 100;
    (secs * 1_000_000_000 + nanos) as i64
}

fn dir_identity(path: &str) -> anyhow::Result<DirId> {
    let wide_path = string_to_wide(path);
    let handle = unsafe {
        CreateFileW(PCWSTR(wide_path.as_ptr()), GENERIC_READ.0, FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE, None, OPEN_EXISTING, FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT, None)?
    };
    if handle == INVALID_HANDLE_VALUE { return Err(anyhow::anyhow!("Invalid handle")); }
    let mut info: BY_HANDLE_FILE_INFORMATION = unsafe { std::mem::zeroed() };
    let result = unsafe { GetFileInformationByHandle(handle, &mut info) };
    unsafe { CloseHandle(handle) }?; result?;
    Ok(DirId { volume_serial: info.dwVolumeSerialNumber, file_index: ((info.nFileIndexHigh as u64) << 32) | (info.nFileIndexLow as u64) })
}

fn cache_key(dir: &str) -> String { dir.to_lowercase().replace('/', "\\") }