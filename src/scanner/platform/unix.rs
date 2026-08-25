//! Unix/macOS-specific file system scanner using jwalk (parallel walk)
//! with rayon for parallel metadata stat.

use rayon::prelude::*;
use std::collections::HashMap;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::SystemTime;

use crate::cache::Cache;
use crate::scanner::platform::Progress;
use crate::scanner_types::{Attrs, CacheEntry, FileRecord, Fingerprint, Options, ScanError};

/// Unix/macOS walker: jwalk parallel directory traversal + rayon parallel stat
pub struct UnixWalker {
    opts: Options,
    progress: Progress,
    cache: Option<Arc<dyn Cache>>,
    exclude_set: std::collections::HashSet<String>,
    pref_lower: Vec<String>,
    stopped: AtomicBool,
}

impl UnixWalker {
    pub fn new(opts: Options, progress: Progress, cache: Option<Arc<dyn Cache>>) -> Self {
        let workers = if opts.workers == 0 {
            num_cpus::get()
        } else {
            opts.workers
        };

        let mut exclude_set = std::collections::HashSet::with_capacity(opts.exclude.len());
        for e in &opts.exclude {
            exclude_set.insert(e.trim_end_matches(['\\', '/']).to_lowercase());
        }
        let mut pref_lower = Vec::with_capacity(opts.exclude_pref.len());
        for p in &opts.exclude_pref {
            pref_lower.push(p.trim_end_matches(['\\', '/']).to_lowercase());
        }

        Self {
            opts: Options { workers, ..opts },
            progress,
            cache,
            exclude_set,
            pref_lower,
            stopped: AtomicBool::new(false),
        }
    }

    pub fn walk(&self, root: &str) -> anyhow::Result<(Vec<FileRecord>, Vec<ScanError>)> {
        let root_path = std::fs::canonicalize(root)?;
        let root_str = root_path.to_string_lossy().to_string();

        if let Some(cache) = &self.cache {
            if let Some(total) = cache.load_total() {
                self.progress.set_total(total as i64);
            }
        }

        // --- Phase 1: parallel traversal (jwalk), collect (parent_dir, file_path) pairs ---
        // skip_hidden(false): dot-directories like ~/.Trash must be scanned —
        // they are exactly where cleanable junk (trash, caches) lives.
        let pairs: Vec<(String, String)> = jwalk::WalkDir::new(&root_str)
            .follow_links(self.opts.follow_links)
            .skip_hidden(false)
            .into_iter()
            .filter_map(|entry| entry.ok())
            .take_while(|_| !self.stopped.load(Ordering::Relaxed))
            .filter_map(|entry| {
                let ft = entry.file_type();
                if ft.is_dir() {
                    self.progress.add_dir();
                    None
                } else if ft.is_file() {
                    let path = entry.path();
                    let path_str = path.to_string_lossy().to_string();
                    if self.is_excluded_path(&path_str) {
                        None
                    } else {
                        let parent = path
                            .parent()
                            .map(|p| p.to_string_lossy().to_string())
                            .unwrap_or_else(|| root_str.clone());
                        Some((parent, path_str))
                    }
                } else {
                    None
                }
            })
            .collect();

        // Group by parent directory
        let mut dir_map: HashMap<String, Vec<String>> = HashMap::new();
        for (parent, path) in pairs {
            dir_map.entry(parent).or_insert_with(Vec::new).push(path);
        }

        let total_files: usize = dir_map.values().map(|v| v.len()).sum();
        self.progress.set_total(total_files as i64);

        // --- Phase 2: parallel stat per directory (with cache support) ---
        let dirs: Vec<String> = dir_map.keys().cloned().collect();

        let results: Vec<(Vec<FileRecord>, Vec<ScanError>, bool)> = dirs
            .par_iter()
            .map(|dir| {
                let files = &dir_map[dir];
                let mut recs = Vec::with_capacity(files.len());
                let mut errs = Vec::new();
                let mut from_cache = false;

                if let Some(cache) = &self.cache {
                    if let Ok(meta) = std::fs::metadata(dir) {
                        let fp = Fingerprint {
                            mod_time_ns: meta
                                .modified()
                                .ok()
                                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                                .map(|d| d.as_nanos() as i64)
                                .unwrap_or(0),
                        };
                        let cache_key = dir.to_lowercase();
                        if let Some(entry) = cache.lookup(&cache_key, &fp) {
                            for f in &entry.files {
                                self.progress.add_file(f.size);
                                recs.push(f.clone());
                            }
                            self.progress.add_cached();
                            from_cache = true;
                        } else {
                            for f in files {
                                if self.stopped.load(Ordering::Relaxed) {
                                    break;
                                }
                                match stat_file(f) {
                                    Ok(rec) => {
                                        self.progress.add_file(rec.size);
                                        recs.push(rec);
                                    }
                                    Err(e) => errs.push(ScanError {
                                        path: f.clone(),
                                        error: e.to_string(),
                                    }),
                                }
                            }
                            if !self.stopped.load(Ordering::Relaxed) {
                                let cache_entry = CacheEntry {
                                    fingerprint: fp,
                                    files: recs.clone(),
                                    dirs: Vec::new(),
                                };
                                let _ = cache.save(&cache_key, cache_entry);
                            }
                        }
                    }
                } else {
                    for f in files {
                        if self.stopped.load(Ordering::Relaxed) {
                            break;
                        }
                        match stat_file(f) {
                            Ok(rec) => {
                                self.progress.add_file(rec.size);
                                recs.push(rec);
                            }
                            Err(e) => errs.push(ScanError {
                                path: f.clone(),
                                error: e.to_string(),
                            }),
                        }
                    }
                }

                (recs, errs, from_cache)
            })
            .collect();

        self.progress.finish();

        let mut recs = Vec::new();
        let mut errs = Vec::new();
        let mut cached_count = 0usize;
        for (r, e, c) in results {
            recs.extend(r);
            errs.extend(e);
            if c {
                cached_count += 1;
            }
        }

        if let Some(cache) = &self.cache {
            let _ = cache.save_total(recs.len() as i64);
        }

        let _ = cached_count;
        let _ = total_files;

        Ok((recs, errs))
    }

    /// Fast exclude check by basename and prefix (lowercase, no per-file allocs beyond one String)
    fn is_excluded_path(&self, path: &str) -> bool {
        if self.exclude_set.is_empty() && self.pref_lower.is_empty() {
            return false;
        }
        let lower = path.to_lowercase();
        for p in &self.pref_lower {
            if lower == *p || lower.starts_with(&format!("{}/", p)) {
                return true;
            }
        }
        if !self.exclude_set.is_empty() {
            if let Some(base) = Path::new(path).file_name().and_then(|s| s.to_str()) {
                if self.exclude_set.contains(&base.to_lowercase()) {
                    return true;
                }
            }
        }
        false
    }

    fn excluded(&self, path: &Path) -> bool {
        self.is_excluded_path(&path.to_string_lossy())
    }

    pub fn stop(&self) {
        self.stopped.store(true, Ordering::Relaxed);
    }
}

/// Stat a single file into a FileRecord
fn stat_file(path: &str) -> anyhow::Result<FileRecord> {
    let metadata = std::fs::metadata(path)?;
    let size = metadata.len() as i64;
    let mod_time = metadata.modified().unwrap_or(SystemTime::now());
    let attrs = Attrs {
        is_dir: false,
        is_reparse: false,
        is_hidden: is_hidden(Path::new(path)),
        is_system: false,
    };
    Ok(FileRecord {
        path: path.to_string(),
        size,
        mod_time,
        attrs,
    })
}

impl crate::scanner::platform::PlatformWalker for UnixWalker {
    fn walk(&self, root: &str) -> anyhow::Result<(Vec<FileRecord>, Vec<ScanError>)> {
        self.walk(root)
    }

    fn stop(&self) {
        self.stop()
    }
}

/// Check if file is hidden on Unix/macOS
fn is_hidden(path: &Path) -> bool {
    path.file_name()
        .and_then(|n| n.to_str())
        .map(|n| n.starts_with('.'))
        .unwrap_or(false)
}
