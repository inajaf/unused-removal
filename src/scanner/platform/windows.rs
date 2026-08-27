//! Windows-specific file system scanner using Win32 API

use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use windows::core::PCWSTR;
use windows::Win32::Foundation::GENERIC_READ;
use windows::Win32::Foundation::{CloseHandle, INVALID_HANDLE_VALUE};
use windows::Win32::Storage::FileSystem::{
    CreateFileW, FindClose, FindFirstFileExW, FindNextFileW, GetFileInformationByHandle,
    BY_HANDLE_FILE_INFORMATION, FILE_ATTRIBUTE_DIRECTORY, FILE_ATTRIBUTE_HIDDEN,
    FILE_ATTRIBUTE_REPARSE_POINT, FILE_ATTRIBUTE_SYSTEM, FILE_FLAG_BACKUP_SEMANTICS,
    FILE_FLAG_OPEN_REPARSE_POINT, FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE,
    FINDEX_INFO_LEVELS, FIND_FIRST_EX_LARGE_FETCH, OPEN_EXISTING,
};

/// Local copy of the canonical Win32_FIND_DATA (Unicode) layout so `FindFirstFileExW` can fill it in place.
/// Field order/types mirror `_WIN32_FIND_DATAW` exactly; `#[repr(C)]` keeps offsets aligned to Windows.
#[repr(C)]
#[derive(Debug, Copy, Clone)]
struct FileTime {
    dwLowDateTime: u32,
    dwHighDateTime: u32,
}

/// Local copy of the canonical Win32_FIND_DATA (Unicode) layout so `FindFirstFileExW` can fill it in place.
/// Field order/types mirror `_WIN32_FIND_DATAW` exactly; `#[repr(C)]` keeps offsets aligned to Windows.
/// Large array fields (>32 elements) don't get a blanket `Default`, and callers use `std::mem::zeroed()` instead.
#[repr(C)]
#[derive(Debug)]
struct FindData {
    dwFileAttributes: u32,
    ftCreationTime: FileTime,
    ftLastAccessTime: FileTime,
    ftLastWriteTime: FileTime,
    nFileSizeHigh: u32,
    nFileSizeLow: u32,
    // Windows inserts reserved/padding fields before cFileName; keeping them preserves offsets.
    dwReserved0: u32,
    dwReserved1: u32,
    cFileName: [u16; 260],
    cAlternateFileName: [u16; 14],
}

use crate::cache::Cache;
use crate::scanner::platform::Progress;
use crate::scanner_types::{Attrs, CacheEntry, DirId, FileRecord, Fingerprint, Options, ScanError};

struct TaskGuard<'a> {
    walker: &'a WindowsWalker,
}

impl<'a> TaskGuard<'a> {
    fn new(walker: &'a WindowsWalker) -> Self {
        Self { walker }
    }
}

impl<'a> Drop for TaskGuard<'a> {
    fn drop(&mut self) {
        // Perform the final decrement and the "no more work" broadcast under the queue mutex.
        // A worker that holds the queue lock, observes total_tasks > 0, and is about to call
        // Condvar::wait() would otherwise MISS a notify_all() fired by another worker finishing
        // the last task (a Condvar has no pending-notify memory). That miss lets the last worker
        // sleep forever while total_tasks is already 0 -> guaranteed multi-worker hang.
        let _queue_guard = self.walker.queue.lock().unwrap();
        let prev = self.walker.total_tasks.load(Ordering::Relaxed);
        if prev > 0 {
            self.walker.total_tasks.fetch_sub(1, Ordering::Relaxed);
            let remaining = self.walker.total_tasks.load(Ordering::Relaxed);
            self.walker.progress.update_discovery(remaining);
            if remaining == 0 {
                self.walker.queue_not_empty.notify_all();
            }
        }
    }
}

/// Windows walker using Win32 FindFirstFileExW
pub struct WindowsWalker {
    opts: Options,
    progress: Progress,
    cache: Option<Arc<dyn Cache>>,
    queue: Arc<Mutex<Vec<String>>>,
    queue_not_empty: Arc<Condvar>,
    workers_done: Arc<Mutex<usize>>,
    // Shared across all worker clones (must be Arc, NOT a plain AtomicU64). If it stayed a plain
    // field, Clone would give every worker its OWN private copy of the counter, so each worker's
    // increment/decrement updated a different variable and the "queue empty && total_tasks == 0"
    // termination predicate never reflected real work — causing both premature "Files: 0" and
    // multi-worker hangs (workers wait forever on a counter that never reaches a shared zero).
    total_tasks: Arc<AtomicU64>,
    recs: Arc<Mutex<Vec<FileRecord>>>,
    errs: Arc<Mutex<Vec<ScanError>>>,
    exclude_lower: Vec<String>,
    pref_lower: Vec<String>,
    seen_dirs: Arc<Mutex<std::collections::HashSet<DirId>>>,
    stopped: Arc<AtomicBool>,
}

impl WindowsWalker {
    pub fn new(opts: Options, progress: Progress, cache: Option<Arc<dyn Cache>>) -> Self {
        let workers = if opts.workers == 0 {
            num_cpus::get()
        } else {
            opts.workers
        };

        let mut exclude_lower = Vec::with_capacity(opts.exclude.len());
        for e in &opts.exclude {
            exclude_lower.push(e.to_lowercase());
        }
        let mut pref_lower = Vec::with_capacity(opts.exclude_pref.len());
        for p in &opts.exclude_pref {
            pref_lower.push(p.trim_end_matches(['\\', '/']).to_lowercase());
        }

        Self {
            opts: Options { workers, ..opts },
            progress,
            cache,
            queue: Arc::new(Mutex::new(Vec::with_capacity(4096))),
            queue_not_empty: Arc::new(Condvar::new()),
            workers_done: Arc::new(Mutex::new(0)),
            total_tasks: Arc::new(AtomicU64::new(0)),
            recs: Arc::new(Mutex::new(Vec::new())),
            errs: Arc::new(Mutex::new(Vec::new())),
            exclude_lower,
            pref_lower,
            seen_dirs: Arc::new(Mutex::new(std::collections::HashSet::new())),
            stopped: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn walk(&self, root: &str) -> anyhow::Result<(Vec<FileRecord>, Vec<ScanError>)> {
        let root_path = std::fs::canonicalize(root)?;
        let root_str = root_path.to_string_lossy().to_string();
        let root_cache_key = cache_key(&root_str);

        // A smart scan may reuse one walker sequentially for multiple Windows volumes. Per-walk
        // buffers must not leak into the next volume or records get duplicated on every pass.
        self.queue.lock().unwrap().clear();
        self.recs.lock().unwrap().clear();
        self.errs.lock().unwrap().clear();
        self.seen_dirs.lock().unwrap().clear();
        *self.workers_done.lock().unwrap() = 0;
        self.total_tasks.store(0, Ordering::Relaxed);

        if let Some(cache) = &self.cache {
            if let Some(total) = cache.load_total(&root_cache_key) {
                self.progress.set_total(total as i64);
            }
        }

        // Seed the root onto the queue and mark one pending task BEFORE spawning workers.
        // Otherwise every worker can grab the empty queue while total_tasks is still 0 and exit
        // immediately via TaskGuard accounting, so `root` never gets processed and EVERY scan
        // reports "Files: 0" regardless of real contents (deterministic for any worker count).
        {
            let mut queue = self.queue.lock().unwrap();
            queue.push(root_str);
            self.total_tasks.fetch_add(1, Ordering::Relaxed);
        }

        let mut handles = Vec::with_capacity(self.opts.workers);
        for _ in 0..self.opts.workers {
            let walker = self.clone();
            handles.push(thread::spawn(move || walker.worker_loop()));
        }
        self.queue_not_empty.notify_all();

        for handle in handles {
            handle.join().unwrap();
        }

        let recs = self.recs.lock().unwrap().clone();
        let errs = self.errs.lock().unwrap().clone();

        if let Some(cache) = &self.cache {
            let _ = cache.save_total(&root_cache_key, recs.len() as i64);
        }

        Ok((recs, errs))
    }

    fn worker_loop(&self) {
        loop {
            let dir = {
                let mut queue = self.queue.lock().unwrap();
                loop {
                    if self.stopped.load(Ordering::Relaxed) {
                        break None;
                    }
                    if let Some(dir) = queue.pop() {
                        break Some(dir);
                    }
                    // Break if no more tasks pending (queue empty and total_tasks == 0)
                    if self.total_tasks.load(Ordering::Relaxed) == 0 {
                        break None;
                    }
                    queue = self.queue_not_empty.wait(queue).unwrap();
                }
            };

            let Some(dir) = dir else {
                let mut done = self.workers_done.lock().unwrap();
                *done += 1;
                if *done == self.opts.workers {
                    self.queue_not_empty.notify_all();
                }
                break;
            };

            self.process_dir(&dir);
        }
    }

    fn process_dir(&self, dir: &str) {
        // Decrement task counter at the start to ensure it's called even on early return
        let _task_guard = TaskGuard::new(self);
        if self.stopped.load(Ordering::Relaxed) {
            return;
        }
        self.progress.add_dir();

        let (fingerprint, entries) = match read_dir_entries(dir) {
            Ok((fp, entries)) => (fp, entries),
            Err(e) => {
                self.add_error(dir.to_string(), e.to_string());
                return;
            }
        };

        if let Some(cache) = &self.cache {
            let cache_key = cache_key(dir);
            if let Some(entry) = cache.lookup(&cache_key, &fingerprint) {
                self.progress.add_cached();
                self.add_cached_entries(&entry);
                for sub in &entry.dirs {
                    let child = format!("{}\\{}", dir.trim_end_matches('\\'), sub);
                    if !self.excluded(&child) {
                        self.push_dir(&child);
                    }
                }
                return;
            }
        }

        let mut files = Vec::new();
        let mut subdirs = Vec::new();

        for entry in entries {
            if self.stopped.load(Ordering::Relaxed) {
                break;
            }
            if entry.is_dir {
                if entry.is_reparse && !self.opts.follow_links {
                    continue;
                }
                let child = format!("{}\\{}", dir.trim_end_matches('\\'), entry.name);
                if self.excluded(&child) {
                    continue;
                }
                if entry.is_reparse && !self.check_follow_cycle(&child) {
                    continue;
                }
                subdirs.push(entry.name);
                self.push_dir(&child);
            } else {
                let path = format!("{}\\{}", dir.trim_end_matches('\\'), entry.name);
                let record = FileRecord {
                    path,
                    size: entry.size,
                    mod_time: entry.mod_time,
                    attrs: entry.attrs,
                };
                files.push(record.clone());
                self.add_record(record);
            }
        }

        if let Some(cache) = &self.cache {
            let cache_key = cache_key(dir);
            let _ = cache.save(
                &cache_key,
                CacheEntry {
                    fingerprint,
                    files,
                    dirs: subdirs,
                },
            );
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
        let mut recs = self.recs.lock().unwrap();
        recs.push(record);
    }

    fn add_error(&self, path: String, error: String) {
        self.progress.add_error();
        self.errs.lock().unwrap().push(ScanError { path, error });
    }

    fn push_dir(&self, dir: &str) {
        if self.stopped.load(Ordering::Relaxed) {
            return;
        }
        // Increment total_tasks under the same queue mutex that workers hold while deciding to
        // wait. Keeps the "queue empty && total_tasks == 0" exit predicate consistent with the
        // Condvar wait (no stale read can trick a worker into exiting while work is still queued).
        let mut queue = self.queue.lock().unwrap();
        self.total_tasks.fetch_add(1, Ordering::Relaxed);
        queue.push(dir.to_string());
        drop(queue);
        self.queue_not_empty.notify_one();
    }

    fn excluded(&self, dir: &str) -> bool {
        if self.pref_lower.is_empty() && self.exclude_lower.is_empty() {
            return false;
        }
        let dir_lower = dir.to_lowercase();
        let dir_trimmed = dir_lower.trim_end_matches(['\\', '/']);
        for p in &self.pref_lower {
            if dir_trimmed == p
                || dir_trimmed.starts_with(&format!("{}\\", p))
                || dir_trimmed.starts_with(&format!("{}/", p))
            {
                return true;
            }
        }
        let base = Path::new(dir)
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("");
        let base_lower = base.to_lowercase();
        for e in &self.exclude_lower {
            if base_lower == *e {
                return true;
            }
        }
        false
    }

    fn check_follow_cycle(&self, path: &str) -> bool {
        if let Ok(id) = dir_identity(path) {
            let mut seen = self.seen_dirs.lock().unwrap();
            if seen.contains(&id) {
                return false;
            }
            seen.insert(id);
            true
        } else {
            true
        }
    }

    pub fn stop(&self) {
        self.stopped.store(true, Ordering::Relaxed);
        self.queue_not_empty.notify_all();
    }
}

impl Clone for WindowsWalker {
    fn clone(&self) -> Self {
        Self {
            opts: self.opts.clone(),
            progress: self.progress.clone(),
            cache: self.cache.clone(),
            queue: self.queue.clone(),
            queue_not_empty: self.queue_not_empty.clone(),
            workers_done: self.workers_done.clone(),
            total_tasks: self.total_tasks.clone(),
            recs: self.recs.clone(),
            errs: self.errs.clone(),
            exclude_lower: self.exclude_lower.clone(),
            pref_lower: self.pref_lower.clone(),
            seen_dirs: self.seen_dirs.clone(),
            stopped: self.stopped.clone(),
        }
    }
}

impl crate::scanner::platform::PlatformWalker for WindowsWalker {
    fn walk(&self, root: &str) -> anyhow::Result<(Vec<FileRecord>, Vec<ScanError>)> {
        self.walk(root)
    }

    fn stop(&self) {
        self.stop()
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
    // FindExInfoStandard (level 0) is the only valid "full info" level and populates cFileName,
    // sizes and attributes. Level 1 (FindExInfoBasic) omits the short (8.3) name, and level 2
    // (FindExInfoMaxInfoLevel) is NOT accepted by FindFirstFileExW — passing it yields
    // ERROR_INVALID_PARAMETER (0x57) and every read_dir_entries() call fails -> Files: 0 on Windows.
    let pattern = build_search_pattern(dir);
    let mut find_data: FindData = unsafe { std::mem::zeroed() };
    let handle = unsafe {
        FindFirstFileExW(
            PCWSTR(pattern.as_ptr()),
            FINDEX_INFO_LEVELS(0), // FindExInfoStandard — fills cFileName, sizes and attributes
            &mut find_data as *mut _ as *mut _,
            windows::Win32::Storage::FileSystem::FINDEX_SEARCH_OPS(0),
            None,
            FIND_FIRST_EX_LARGE_FETCH,
        )?
    };

    let mut fingerprint_hasher = std::collections::hash_map::DefaultHasher::new();
    let mut entries = Vec::new();

    loop {
        let name = wide_to_string(&find_data.cFileName);
        if name != "." && name != ".." {
            // Directory mtime alone is not enough on Windows: editing an existing file can leave
            // the parent timestamp unchanged. Hash entry name, size, attributes and write time so
            // cached scans cannot replay stale sizes or miss newly relevant large files.
            name.to_lowercase().hash(&mut fingerprint_hasher);
            find_data.dwFileAttributes.hash(&mut fingerprint_hasher);
            find_data.nFileSizeHigh.hash(&mut fingerprint_hasher);
            find_data.nFileSizeLow.hash(&mut fingerprint_hasher);
            find_data
                .ftLastWriteTime
                .dwHighDateTime
                .hash(&mut fingerprint_hasher);
            find_data
                .ftLastWriteTime
                .dwLowDateTime
                .hash(&mut fingerprint_hasher);
            let attrs = win32_attrs_to_attrs(find_data.dwFileAttributes);
            entries.push(WinEntry {
                name,
                size: ((find_data.nFileSizeHigh as i64) << 32) | (find_data.nFileSizeLow as i64),
                mod_time: filetime_to_system_time(find_data.ftLastWriteTime),
                is_dir: attrs.is_dir,
                is_reparse: attrs.is_reparse,
                attrs,
            });
        }
        if let Err(err) = unsafe { FindNextFileW(handle, &mut find_data as *mut _ as *mut _) } {
            if err.code() == windows::Win32::Foundation::ERROR_NO_MORE_FILES.to_hresult() {
                break;
            }
            let _ = unsafe { FindClose(handle) };
            return Err(err.into());
        }
    }
    unsafe { FindClose(handle) }?;
    let fingerprint = Fingerprint {
        mod_time_ns: fingerprint_hasher.finish() as i64,
    };
    Ok((fingerprint, entries))
}

fn build_search_pattern(dir: &str) -> Vec<u16> {
    let mut path = PathBuf::from(dir);
    path.push("*");
    let path_str = path.to_string_lossy().to_string();
    let final_path = if path_str.len() > 248 && !path_str.starts_with(r"\\?\") {
        format!(r"\\?\{}", path_str)
    } else {
        path_str
    };
    string_to_wide(&final_path)
}

fn string_to_wide(s: &str) -> Vec<u16> {
    use std::os::windows::ffi::OsStrExt;
    std::ffi::OsStr::new(s)
        .encode_wide()
        .chain(Some(0))
        .collect()
}

fn wide_to_string(slice: &[u16]) -> String {
    let len = slice.iter().position(|&c| c == 0).unwrap_or(slice.len());
    String::from_utf16_lossy(&slice[..len])
}

fn win32_attrs_to_attrs(attrs: u32) -> Attrs {
    Attrs {
        is_dir: attrs & FILE_ATTRIBUTE_DIRECTORY.0 != 0,
        is_reparse: attrs & FILE_ATTRIBUTE_REPARSE_POINT.0 != 0,
        is_hidden: attrs & FILE_ATTRIBUTE_HIDDEN.0 != 0,
        is_system: attrs & FILE_ATTRIBUTE_SYSTEM.0 != 0,
    }
}

fn filetime_to_system_time(ft: FileTime) -> SystemTime {
    let ticks = ((ft.dwHighDateTime as u64) << 32) | (ft.dwLowDateTime as u64);
    const TICKS_PER_SEC: u64 = 10_000_000;
    const UNIX_EPOCH_OFFSET: u64 = 116_444_736_000_000_000;
    let unix_ticks = ticks.saturating_sub(UNIX_EPOCH_OFFSET);
    let secs = unix_ticks / TICKS_PER_SEC;
    let nanos = ((unix_ticks % TICKS_PER_SEC) * 100) as u32;
    UNIX_EPOCH + Duration::new(secs, nanos)
}

fn dir_identity(path: &str) -> anyhow::Result<DirId> {
    let wide_path = string_to_wide(path);
    let handle = unsafe {
        CreateFileW(
            PCWSTR(wide_path.as_ptr()),
            GENERIC_READ.0,
            FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
            None,
            OPEN_EXISTING,
            FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT,
            None,
        )?
    };
    if handle == INVALID_HANDLE_VALUE {
        return Err(anyhow::anyhow!("Invalid handle"));
    }
    let mut info: BY_HANDLE_FILE_INFORMATION = unsafe { std::mem::zeroed() };
    let result = unsafe { GetFileInformationByHandle(handle, &mut info) };
    unsafe { CloseHandle(handle) }?;
    result?;
    Ok(DirId {
        volume_serial: info.dwVolumeSerialNumber,
        file_index: ((info.nFileIndexHigh as u64) << 32) | (info.nFileIndexLow as u64),
    })
}

fn cache_key(dir: &str) -> String {
    dir.to_lowercase().replace('/', "\\")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn scans_nested_files_with_native_win32_walker() {
        let root = tempfile::tempdir().unwrap();
        let nested = root.path().join("nested");
        fs::create_dir(&nested).unwrap();
        fs::write(root.path().join("top.bin"), b"top").unwrap();
        fs::write(nested.join("large.bin"), vec![0_u8; 4096]).unwrap();
        let walker = WindowsWalker::new(Options::default(), Progress::new(), None);

        let (records, errors) = walker.walk(&root.path().to_string_lossy()).unwrap();

        assert!(errors.is_empty(), "{errors:?}");
        assert_eq!(records.len(), 2);
        assert!(records.iter().any(|record| record.size == 4096));
    }

    #[test]
    fn sequential_roots_do_not_return_cumulative_records() {
        let first = tempfile::tempdir().unwrap();
        let second = tempfile::tempdir().unwrap();
        fs::write(first.path().join("first.bin"), b"first").unwrap();
        fs::write(second.path().join("second.bin"), b"second").unwrap();
        let walker = WindowsWalker::new(Options::default(), Progress::new(), None);

        let (first_records, _) = walker.walk(&first.path().to_string_lossy()).unwrap();
        let (second_records, _) = walker.walk(&second.path().to_string_lossy()).unwrap();

        assert_eq!(first_records.len(), 1);
        assert_eq!(second_records.len(), 1);
        assert!(second_records[0].path.to_lowercase().contains("second.bin"));
    }
}
