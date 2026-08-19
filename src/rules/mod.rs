//! Classification rules engine for categorizing files

use std::path::Path;
use std::sync::Arc;
use rayon::prelude::*;
use blake3;
use std::io::{Read, BufReader};
use std::fs::File;
use anyhow::Result;

use crate::config::Config;
use crate::scanner_types::FileRecord;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Category {
    Huge,
    Large,
    Junk,
    Stale,
    OldLog,
    StaleInstall,
    Duplicate,
}

impl std::fmt::Display for Category {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Category::Huge => write!(f, "huge"),
            Category::Large => write!(f, "large"),
            Category::Junk => write!(f, "junk"),
            Category::Stale => write!(f, "stale"),
            Category::OldLog => write!(f, "old_log"),
            Category::StaleInstall => write!(f, "stale_install"),
            Category::Duplicate => write!(f, "duplicate"),
        }
    }
}

impl std::str::FromStr for Category {
    type Err = &'static str;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "huge" => Ok(Category::Huge),
            "large" => Ok(Category::Large),
            "junk" => Ok(Category::Junk),
            "stale" => Ok(Category::Stale),
            "old_log" => Ok(Category::OldLog),
            "stale_install" => Ok(Category::StaleInstall),
            "duplicate" => Ok(Category::Duplicate),
            _ => Err("invalid category"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Risk {
    Safe,
    Caution,
    Protected,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Finding {
    pub path: String,
    pub size: i64,
    pub category: Category,
    pub reason: String,
    pub risk: Risk,
    #[serde(serialize_with = "serialize_system_time")]
    pub mod_time: std::time::SystemTime,
    pub extra: Option<std::collections::HashMap<String, String>>,
}

fn serialize_system_time<S>(t: &std::time::SystemTime, s: S) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    let dt: chrono::DateTime<chrono::Utc> = t.clone().into();
    s.serialize_str(&dt.to_rfc3339())
}

impl Finding {
    pub fn new(path: String, size: i64, category: Category, reason: String, risk: Risk, mod_time: std::time::SystemTime) -> Self {
        Self {
            path,
            size,
            category,
            reason,
            risk,
            mod_time,
            extra: None,
        }
    }

    pub fn with_extra(mut self, extra: std::collections::HashMap<String, String>) -> Self {
        self.extra = Some(extra);
        self
    }
}

/// Protected system paths that should never be deleted by default
#[cfg(windows)]
const PROTECTED_PATHS: &[&str] = &[
    r"c:\windows\winsxs\",
    r"c:\windows\system32\",
    r"c:\windows\syswow64\",
    r"c:\windows\servicing\",
    r"c:\program files\",
    r"c:\program files (x86)\",
    r"c:\pagefile.sys",
    r"c:\hiberfil.sys",
    r"c:\swapfile.sys",
    r"c:\bootmgr",
    r"c:\boot\",
    r"c:\windows\boot\",
    r"c:\recovery\",
    r"c:\system volume information\",
    r"c:\$recycle.bin\",
];

#[cfg(target_os = "macos")]
const PROTECTED_PATHS: &[&str] = &[
    "/system/",
    "/library/",
    "/usr/",
    "/bin/",
    "/sbin/",
    "/private/var/",
    "/private/etc/",
    "/applications/",
];

#[cfg(all(unix, not(target_os = "macos")))]
const PROTECTED_PATHS: &[&str] = &[
    "/boot/",
    "/etc/",
    "/bin/",
    "/sbin/",
    "/lib/",
    "/lib64/",
    "/usr/",
    "/proc/",
    "/sys/",
    "/dev/",
];

pub fn is_protected(path: &str) -> bool {
    let lower = path.to_lowercase();
    PROTECTED_PATHS.iter().any(|p| lower.starts_with(p))
}

/// Rules engine for classifying files
pub struct Engine {
    config: Arc<Config>,
}

impl Engine {
    pub fn new(config: Arc<Config>) -> Self {
        Self { config }
    }

    /// Analyze all records and return findings
    pub fn analyze(&self, records: &[FileRecord]) -> Vec<Finding> {
        // First pass: classify each file (parallel)
        let findings: Vec<Option<Finding>> = records.par_iter()
            .filter(|r| !r.attrs.is_dir)
            .map(|rec| self.classify_single(rec))
            .collect();

        // Flatten and deduplicate (first match wins based on priority)
        let mut seen = std::collections::HashSet::new();
        let mut results = Vec::new();
        
        for f in findings.into_iter().flatten() {
            if seen.insert(f.path.clone()) {
                results.push(f);
            }
        }

        // Filter protected paths
        if !self.config.allow_protected {
            results = self.filter_protected(results);
        }

        results
    }

    /// Classify a single file record - returns first matching rule (priority order)
    fn classify_single(&self, rec: &FileRecord) -> Option<Finding> {
        // Priority order: Junk > Stale > Huge/Large > OldLog > StaleInstall
        if let Some(f) = self.check_junk(rec) {
            return Some(f);
        }
        if let Some(f) = self.check_stale(rec) {
            return Some(f);
        }
        if let Some(f) = self.check_large(rec) {
            return Some(f);
        }
        if let Some(f) = self.check_old_log(rec) {
            return Some(f);
        }
        if let Some(f) = self.check_stale_install(rec) {
            return Some(f);
        }
        None
    }

    fn check_junk(&self, rec: &FileRecord) -> Option<Finding> {
        let name = Path::new(&rec.path)
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("");
        let lower_name = name.to_lowercase();
        let lower_path = rec.path.to_lowercase();

        // 1) Junk extensions
        for ext in &self.config.junk_extensions {
            if ext == "~$*" {
                if lower_name.starts_with("~$") {
                    return Some(Finding::new(
                        rec.path.clone(),
                        rec.size,
                        Category::Junk,
                        "временный файл Office (~$*)".to_string(),
                        Risk::Safe,
                        rec.mod_time,
                    ));
                }
                continue;
            }
            if lower_name.ends_with(&ext.to_lowercase()) {
                return Some(Finding::new(
                    rec.path.clone(),
                    rec.size,
                    Category::Junk,
                    format!("расширение {}", ext),
                    Risk::Safe,
                    rec.mod_time,
                ));
            }
        }

        // 2) Junk directories
        for jd in &self.config.junk_dirs {
            let jd_lower = jd.to_lowercase();
            if lower_path.starts_with(&format!("{}\\", jd_lower)) || lower_path.starts_with(&format!("{}/", jd_lower)) {
                return Some(Finding::new(
                    rec.path.clone(),
                    rec.size,
                    Category::Junk,
                    format!("в мусорном каталоге: {}", jd),
                    Risk::Safe,
                    rec.mod_time,
                ));
            }
        }

        None
    }

    fn check_stale(&self, rec: &FileRecord) -> Option<Finding> {
        if rec.mod_time < self.config.stale_cutoff().into() {
            Some(Finding::new(
                rec.path.clone(),
                rec.size,
                Category::Stale,
                format!("не менялся > {} дней", self.config.stale_days),
                Risk::Caution,
                rec.mod_time,
            ))
        } else {
            None
        }
    }

    fn check_large(&self, rec: &FileRecord) -> Option<Finding> {
        if rec.size as u64 >= self.config.huge_bytes {
            Some(Finding::new(
                rec.path.clone(),
                rec.size,
                Category::Huge,
                format!("очень крупный файл (> {})", format_bytes(self.config.huge_bytes)),
                Risk::Caution,
                rec.mod_time,
            ))
        } else if rec.size as u64 >= self.config.large_bytes {
            Some(Finding::new(
                rec.path.clone(),
                rec.size,
                Category::Large,
                format!("крупный файл (> {})", format_bytes(self.config.large_bytes)),
                Risk::Caution,
                rec.mod_time,
            ))
        } else {
            None
        }
    }

    fn check_old_log(&self, rec: &FileRecord) -> Option<Finding> {
        let name = Path::new(&rec.path)
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("");
        let lower_name = name.to_lowercase();

        if lower_name.ends_with(".log") && rec.mod_time < self.config.old_log_cutoff().into() {
            Some(Finding::new(
                rec.path.clone(),
                rec.size,
                Category::OldLog,
                format!("старый лог (> {} дней)", self.config.old_log_days),
                Risk::Safe,
                rec.mod_time,
            ))
        } else {
            None
        }
    }

    fn check_stale_install(&self, rec: &FileRecord) -> Option<Finding> {
        let name = Path::new(&rec.path)
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("");
        let lower_name = name.to_lowercase();
        let lower_path = rec.path.to_lowercase();

        if (lower_name.ends_with(".msi") || lower_name.ends_with(".exe") || lower_name.ends_with(".msu"))
            && lower_path.contains(r"\downloads\")
            && rec.mod_time < self.config.stale_install_cutoff().into()
        {
            Some(Finding::new(
                rec.path.clone(),
                rec.size,
                Category::StaleInstall,
                format!("старый инсталлятор в Downloads (> {} дней)", self.config.stale_install_days),
                Risk::Caution,
                rec.mod_time,
            ))
        } else {
            None
        }
    }

    /// Filter out protected paths
    pub fn filter_protected(&self, findings: Vec<Finding>) -> Vec<Finding> {
        findings.into_iter()
            .filter(|f| !is_protected(&f.path))
            .collect()
    }

    /// Find duplicate files using parallel Blake3 hashing
    pub fn find_duplicates(&self, records: &[FileRecord]) -> Vec<Finding> {
        if !self.config.check_duplicates {
            return Vec::new();
        }

        // Group by size first
        let mut by_size: std::collections::HashMap<u64, Vec<&FileRecord>> = std::collections::HashMap::new();
        for rec in records {
            if rec.attrs.is_dir {
                continue;
            }
            if is_protected(&rec.path) {
                continue;
            }
            by_size.entry(rec.size as u64).or_default().push(rec);
        }

        // Collect candidates: files with same size (at least 2)
        let mut candidates = Vec::new();
        for (size, group) in by_size {
            if group.len() >= 2 && size > 0 {
                candidates.extend(group);
            }
        }

        if candidates.len() < 2 {
            return Vec::new();
        }

        // Parallel hashing
        type Hashed = (FileRecord, String);
        let results: Vec<Hashed> = candidates.par_iter()
            .filter_map(|rec| {
                hash_file(&rec.path).ok().map(|h| ((*rec).clone(), h))
            })
            .collect();

        // Group by hash
        let mut by_hash: std::collections::HashMap<String, Vec<Hashed>> = std::collections::HashMap::new();
        for (rec, hash) in results {
            by_hash.entry(hash.clone()).or_default().push((rec, hash));
        }

        // First file in each hash group is "original", rest are duplicates
        let mut findings = Vec::new();
        for (_, group) in by_hash {
            if group.len() < 2 {
                continue;
            }
            let first = &group[0].0;
            for (dup, _) in &group[1..] {
                let mut extra = std::collections::HashMap::new();
                extra.insert("original".to_string(), first.path.clone());
                findings.push(Finding::new(
                    dup.path.clone(),
                    dup.size,
                    Category::Duplicate,
                    format!("дубликат файла {}", Path::new(&first.path).file_name().and_then(|s| s.to_str()).unwrap_or("")),
                    Risk::Caution,
                    dup.mod_time,
                ).with_extra(extra));
            }
        }

        findings
    }
}

/// Compute Blake3 hash of a file
fn hash_file(path: &str) -> Result<String> {
    let file = File::open(path)?;
    let mut reader = BufReader::new(file);
    let mut hasher = blake3::Hasher::new();
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let n = reader.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        hasher.update(&buffer[..n]);
    }
    Ok(hex::encode(hasher.finalize().as_bytes()))
}

fn format_bytes(bytes: u64) -> String {
    const UNIT: u64 = 1024;
    if bytes < UNIT {
        return format!("{} B", bytes);
    }
    let mut div = UNIT;
    let mut exp = 0;
    let mut n = bytes / UNIT;
    while n >= UNIT {
        div *= UNIT;
        exp += 1;
        n /= UNIT;
    }
    format!("{} {}iB", bytes / div, "KMGTPE".chars().nth(exp).unwrap())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_protected() {
        #[cfg(windows)]
        {
            assert!(is_protected(r"C:\Windows\System32\kernel32.dll"));
            assert!(is_protected(r"C:\Program Files\App\file.exe"));
            assert!(!is_protected(r"C:\Users\User\file.txt"));
        }
        #[cfg(target_os = "macos")]
        {
            assert!(is_protected("/System/Library/CoreServices/Finder.app"));
            assert!(is_protected("/usr/bin/python3"));
            assert!(!is_protected("/Users/eldar/Documents/file.txt"));
        }
        #[cfg(all(unix, not(target_os = "macos")))]
        {
            assert!(is_protected("/etc/passwd"));
            assert!(is_protected("/usr/bin/cat"));
            assert!(!is_protected("/home/user/file.txt"));
        }
    }

    #[test]
    fn test_format_bytes() {
        assert_eq!(format_bytes(512), "512 B");
        assert_eq!(format_bytes(1024), "1 KiB");
        assert_eq!(format_bytes(1024 * 1024), "1 MiB");
        assert_eq!(format_bytes(1024 * 1024 * 1024), "1 GiB");
    }
}