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
    AppLeftovers,
    // Smart Junk categories
    UserCache,
    SystemLog,
    LanguageFile,
    OldBackup,
    MailAttachment,
    Trash,
    OldDownload,
    UnusedDiskImage,
    DevCache,
    XcodeCache,
    VSCodeCache,
    LargeHidden,
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
            Category::AppLeftovers => write!(f, "app_leftovers"),
            Category::UserCache => write!(f, "user_cache"),
            Category::SystemLog => write!(f, "system_log"),
            Category::LanguageFile => write!(f, "language_file"),
            Category::OldBackup => write!(f, "old_backup"),
            Category::MailAttachment => write!(f, "mail_attachment"),
            Category::Trash => write!(f, "trash"),
            Category::OldDownload => write!(f, "old_download"),
            Category::UnusedDiskImage => write!(f, "unused_disk_image"),
            Category::DevCache => write!(f, "dev_cache"),
            Category::XcodeCache => write!(f, "xcode_cache"),
            Category::VSCodeCache => write!(f, "vscode_cache"),
            Category::LargeHidden => write!(f, "large_hidden"),
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
            "app_leftovers" => Ok(Category::AppLeftovers),
            "user_cache" => Ok(Category::UserCache),
            "system_log" => Ok(Category::SystemLog),
            "language_file" => Ok(Category::LanguageFile),
            "old_backup" => Ok(Category::OldBackup),
            "mail_attachment" => Ok(Category::MailAttachment),
            "trash" => Ok(Category::Trash),
            "old_download" => Ok(Category::OldDownload),
            "unused_disk_image" => Ok(Category::UnusedDiskImage),
            "dev_cache" => Ok(Category::DevCache),
            "xcode_cache" => Ok(Category::XcodeCache),
            "vscode_cache" => Ok(Category::VSCodeCache),
            "large_hidden" => Ok(Category::LargeHidden),
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
    // Smart Junk protected paths - active development tools
    r"c:\program files\jetbrains\",
    r"c:\program files\microsoft visual studio\",
    r"c:\program files\dotnet\",
    r"c:\program files\nodejs\",
    r"c:\program files\git\",
    r"c:\program files\docker\",
    r"c:\program files\postgresql\",
    r"c:\program files\mongodb\",
    r"c:\users\*\appdata\local\jetbrains\",
    r"c:\users\*\appdata\roaming\jetbrains\",
    r"c:\users\*\appdata\roaming\code\",
    r"c:\users\*\appdata\roaming\cursor\",
    r"c:\users\*\appdata\local\github desktop\",
    r"c:\users\*\appdata\local\microsoft\vscode\",
    r"c:\users\*\appdata\local\github\copilot\",
    r"c:\users\*\appdata\local\microsoft\windowsapps\",
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
    // Smart Junk protected paths - active development tools
    "/library/developer/",
    "/library/caches/com.apple.dt.xcode/",
    "/library/caches/jetbrains/",
    "/library/logs/jetbrains/",
    "/library/application support/jetbrains/",
    "/library/application support/code/",
    "/library/application support/cursor/",
    "/library/application support/github copilot/",
    "/library/application support/github desktop/",
    "/opt/homebrew/",
    "/usr/local/",
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
    // Smart Junk protected paths - active development tools
    "/opt/jetbrains/",
    "/opt/visual-studio-code/",
    "/opt/cursor/",
    "/usr/lib/jetbrains/",
    "/usr/share/jetbrains/",
    "/var/lib/flatpak/",
    "/var/lib/snapd/",
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
        // Priority order: Junk > Smart Junk (high confidence) > Stale > Huge/Large > OldLog > StaleInstall > AppLeftovers > Smart Junk (lower confidence)
        if let Some(f) = self.check_junk(rec) {
            return Some(f);
        }
        // High-confidence smart junk (Safe, metadata-only detection)
        if let Some(f) = self.check_user_cache(rec) {
            return Some(f);
        }
        if let Some(f) = self.check_system_log(rec) {
            return Some(f);
        }
        if let Some(f) = self.check_trash(rec) {
            return Some(f);
        }
        if let Some(f) = self.check_old_download(rec) {
            return Some(f);
        }
        if let Some(f) = self.check_dev_cache(rec) {
            return Some(f);
        }
        if let Some(f) = self.check_xcode_cache(rec) {
            return Some(f);
        }
        if let Some(f) = self.check_vscode_cache(rec) {
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
        if let Some(f) = self.check_app_leftovers(rec) {
            return Some(f);
        }
        // Lower-confidence smart junk (may need content verification)
        if let Some(f) = self.check_mail_attachment(rec) {
            return Some(f);
        }
        if let Some(f) = self.check_old_backup(rec) {
            return Some(f);
        }
        if let Some(f) = self.check_unused_disk_image(rec) {
            return Some(f);
        }
        if let Some(f) = self.check_language_file(rec) {
            return Some(f);
        }
        if let Some(f) = self.check_large_hidden(rec) {
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


    fn check_app_leftovers(&self, rec: &FileRecord) -> Option<Finding> {
        let lower_path = rec.path.to_lowercase();
        let name = Path::new(&rec.path)
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("");
        let lower_name = name.to_lowercase();

        #[cfg(target_os = "macos")]
        {
            // Common macOS app leftover locations
            const APP_LEFTOVER_PATHS: &[&str] = &[
                "/library/preferences/",
                "/library/application support/",
                "/library/caches/",
                "/library/launchagents/",
                "/library/launchdaemons/",
                "/library/preferences/byhost/",
                "/library/saved application state/",
                "/library/containers/",
                "/library/group containers/",
                "/library/saved application state/",
                "/library/logs/",
                "/library/logs/diagnosticreports/",
                "/library/caches/com.apple.",
                "/library/preferences/com.apple.",
                "/private/var/folders/",
                "/var/folders/",
            ];

            // Check if file is in a known app leftover location
            for leftover_path in APP_LEFTOVER_PATHS {
                if lower_path.contains(leftover_path) {
                    // Common patterns for app leftovers
                    let leftover_patterns = [
                        ".plist",
                        ".cache",
                        ".log",
                        ".db",
                        ".sqlite",
                        "saved application state",
                        "savedapplicationstate",
                        "com.apple.",
                        "com.",
                        "org.",
                        "net.",
                        "io.",
                    ];

                    for pattern in &leftover_patterns {
                        if lower_path.contains(pattern) || lower_name.contains(pattern) {
                            return Some(Finding::new(
                                rec.path.clone(),
                                rec.size,
                                Category::AppLeftovers,
                                format!("следы удалённого приложения ({})", pattern),
                                Risk::Safe,
                                rec.mod_time,
                            ).with_extra({
                                let mut extra = std::collections::HashMap::new();
                                extra.insert("detection_type".to_string(), "app_leftover".to_string());
                                extra.insert("pattern".to_string(), pattern.to_string());
                                extra
                            }));
                        }
                    }
                }
            }
        }

        #[cfg(not(target_os = "macos"))]
        {
            // On Linux/Windows, check common leftover locations
            const LEFTOVER_PATHS: &[&str] = &[
                "/.config/",
                "/.local/share/",
                "/.cache/",
                "/.local/state/",
                "/appdata/local/",
                "/appdata/roaming/",
                "/programdata/",
            ];

            for leftover_path in LEFTOVER_PATHS {
                if lower_path.contains(leftover_path) {
                    return Some(Finding::new(
                        rec.path.clone(),
                        rec.size,
                        Category::AppLeftovers,
                        "возможные следы удалённого приложения".to_string(),
                        Risk::Safe,
                        rec.mod_time,
                    ));
                }
            }
        }

        None
    }

    // ========== Smart Junk Detection Methods ==========

    /// Check for user cache files (browser, app caches)
    fn check_user_cache(&self, rec: &FileRecord) -> Option<Finding> {
        if !self.config.scan_user_caches {
            return None;
        }
        let lower_path = rec.path.to_lowercase();
        let _name = Path::new(&rec.path)
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("");
        let _lower_name = _name.to_lowercase();

        // Browser caches (cross-platform patterns)
        const BROWSER_CACHE_PATTERNS: &[&str] = &[
            "/google/chrome/user data/default/cache/",
            "/microsoft/edge/user data/default/cache/",
            "/mozilla/firefox/profiles/",
            "/safari/cache/",
            "/brave/user data/default/cache/",
            "/opera/cache/",
            "/vivaldi/user data/default/cache/",
            "/chromium/user data/default/cache/",
            "appdata/local/google/chrome/user data/default/cache/",
            "appdata/local/microsoft/edge/user data/default/cache/",
            "appdata/local/mozilla/firefox/profiles/",
            "appdata/local/brave/brave/user data/default/cache/",
            "appdata/local/opera/opera/cache/",
            "appdata/local/vivaldi/user data/default/cache/",
        ];

        for pattern in BROWSER_CACHE_PATTERNS {
            if lower_path.contains(pattern) {
                return Some(Finding::new(
                    rec.path.clone(),
                    rec.size,
                    Category::UserCache,
                    format!("браузерный кэш ({})", pattern.split('/').nth(1).unwrap_or("browser")),
                    Risk::Safe,
                    rec.mod_time,
                ));
            }
        }

        // System/user cache directories (macOS)
        #[cfg(target_os = "macos")]
        {
            const MACOS_CACHE_DIRS: &[&str] = &[
                "/library/caches/",
                "/library/caches/com.apple.",
                "/private/var/folders/",
                "/var/folders/",
            ];
            for dir in MACOS_CACHE_DIRS {
                if lower_path.contains(dir) {
                    return Some(Finding::new(
                        rec.path.clone(),
                        rec.size,
                        Category::UserCache,
                        "системный/пользовательский кэш macOS".to_string(),
                        Risk::Safe,
                        rec.mod_time,
                    ));
                }
            }
        }

        // System/user cache directories (Windows)
        #[cfg(target_os = "windows")]
        {
            const WINDOWS_CACHE_DIRS: &[&str] = &[
                "appdata/local/temp/",
                "appdata/local/microsoft/windows/inetcache/",
                "appdata/local/microsoft/windows/webcache/",
                "locallow/",
                "programdata/microsoft/windows/caches/",
                "windows/temp/",
                "windows/prefetch/",
            ];
            for dir in WINDOWS_CACHE_DIRS {
                if lower_path.contains(dir) {
                    return Some(Finding::new(
                        rec.path.clone(),
                        rec.size,
                        Category::UserCache,
                        "системный/пользовательский кэш Windows".to_string(),
                        Risk::Safe,
                        rec.mod_time,
                    ));
                }
            }
        }

        // Generic cache directories
        const GENERIC_CACHE_DIRS: &[&str] = &[
            "/.cache/",
            "/cache/",
            "/caches/",
            "/tmp/",
            "/var/tmp/",
            "/temp/",
            "appdata/local/temp/",
            "appdata/roaming/*/cache/",
        ];

        for dir in GENERIC_CACHE_DIRS {
            if lower_path.contains(dir) && rec.size >= self.config.min_cache_size_bytes() as i64 {
                return Some(Finding::new(
                    rec.path.clone(),
                    rec.size,
                    Category::UserCache,
                    "кэш приложения".to_string(),
                    Risk::Safe,
                    rec.mod_time,
                ));
            }
        }

        None
    }

    /// Check for system log files
    fn check_system_log(&self, rec: &FileRecord) -> Option<Finding> {
        if !self.config.scan_system_logs {
            return None;
        }
        let lower_path = rec.path.to_lowercase();
        let name = Path::new(&rec.path)
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("");
        let lower_name = name.to_lowercase();

        // Log file extensions
        if (lower_name.ends_with(".log") || lower_name.ends_with(".log.old") || lower_name.contains(".log."))
            && rec.mod_time < self.config.old_log_cutoff().into()
        {
            // Exclude active application logs
            if lower_path.contains("/library/logs/") || lower_path.contains("appdata/local/") && lower_path.contains("log") {
                return Some(Finding::new(
                    rec.path.clone(),
                    rec.size,
                    Category::SystemLog,
                    format!("старый системный лог (> {} дней)", self.config.old_log_days),
                    Risk::Safe,
                    rec.mod_time,
                ));
            }
        }

        // macOS system logs
        #[cfg(target_os = "macos")]
        {
            const MACOS_LOG_DIRS: &[&str] = &[
                "/var/log/",
                "/library/logs/",
                "/library/logs/diagnosticreports/",
                "/private/var/log/",
            ];
            for dir in MACOS_LOG_DIRS {
                if lower_path.contains(dir) {
                    return Some(Finding::new(
                        rec.path.clone(),
                        rec.size,
                        Category::SystemLog,
                        "системный лог macOS".to_string(),
                        Risk::Safe,
                        rec.mod_time,
                    ));
                }
            }
        }

        // Windows event logs
        #[cfg(target_os = "windows")]
        {
            const WINDOWS_LOG_DIRS: &[&str] = &[
                "windows/system32/winevt/logs/",
                "windows/logs/",
                "windows/debug/",
                "programdata/microsoft/windows/winsat/",
            ];
            for dir in WINDOWS_LOG_DIRS {
                if lower_path.contains(dir) {
                    return Some(Finding::new(
                        rec.path.clone(),
                        rec.size,
                        Category::SystemLog,
                        "системный лог Windows".to_string(),
                        Risk::Safe,
                        rec.mod_time,
                    ));
                }
            }
        }

        None
    }

    /// Check for Trash/Recycle Bin contents
    fn check_trash(&self, rec: &FileRecord) -> Option<Finding> {
        if !self.config.scan_trash {
            return None;
        }
        let lower_path = rec.path.to_lowercase();

        #[cfg(target_os = "macos")]
        {
            if lower_path.contains("/.trash/") || lower_path.contains("/.trashes/") {
                return Some(Finding::new(
                    rec.path.clone(),
                    rec.size,
                    Category::Trash,
                    "корзина macOS".to_string(),
                    Risk::Safe,
                    rec.mod_time,
                ));
            }
        }

        #[cfg(target_os = "windows")]
        {
            if lower_path.contains("$recycle.bin") || lower_path.contains("recycler") {
                return Some(Finding::new(
                    rec.path.clone(),
                    rec.size,
                    Category::Trash,
                    "корзина Windows".to_string(),
                    Risk::Safe,
                    rec.mod_time,
                ));
            }
        }

        #[cfg(all(unix, not(target_os = "macos")))]
        {
            if lower_path.contains("/.local/share/trash/") || lower_path.contains("/.trash/") {
                return Some(Finding::new(
                    rec.path.clone(),
                    rec.size,
                    Category::Trash,
                    "корзина Linux".to_string(),
                    Risk::Safe,
                    rec.mod_time,
                ));
            }
        }

        None
    }

    /// Check for old downloads
    fn check_old_download(&self, rec: &FileRecord) -> Option<Finding> {
        if !self.config.scan_old_downloads {
            return None;
        }
        let lower_path = rec.path.to_lowercase();
        let cutoff = chrono::Utc::now() - chrono::Duration::days(self.config.old_download_days);

        // Check if in Downloads folder
        let in_downloads = lower_path.contains("/downloads/") || lower_path.contains("\\downloads\\");

        if in_downloads && rec.mod_time < cutoff.into() {
            return Some(Finding::new(
                rec.path.clone(),
                rec.size,
                Category::OldDownload,
                format!("старый файл в Downloads (> {} дней)", self.config.old_download_days),
                Risk::Safe,
                rec.mod_time,
            ));
        }

        None
    }

    /// Check for development caches (npm, cargo, pip, gradle, maven, go, bun, pnpm, yarn)
    fn check_dev_cache(&self, rec: &FileRecord) -> Option<Finding> {
        if !self.config.scan_dev_caches {
            return None;
        }
        let lower_path = rec.path.to_lowercase();

        const DEV_CACHE_PATTERNS: &[(&str, &str)] = &[
            ("/.cargo/registry/cache/", "Cargo registry cache"),
            ("/.cargo/git/checkouts/", "Cargo git checkouts"),
            ("/target/", "Cargo build artifacts"),
            ("/node_modules/.cache/", "Node.js cache"),
            ("/.npm/", "npm cache"),
            ("/node_modules/.vite/", "Vite cache"),
            ("/node_modules/.parcel-cache/", "Parcel cache"),
            ("/node_modules/.turbo/", "Turborepo cache"),
            ("/.pnpm-store/", "pnpm store"),
            ("/.yarn/cache/", "Yarn cache"),
            ("/.bun/cache/", "Bun cache"),
            ("/.gradle/caches/", "Gradle cache"),
            ("/.gradle/daemon/", "Gradle daemon"),
            ("/.m2/repository/", "Maven repository"),
            ("/pip/cache/", "pip cache"),
            ("/pip/wheel/", "pip wheel cache"),
            ("/~/.cache/pip/", "pip cache (legacy)"),
            ("/go/pkg/mod/", "Go module cache"),
            ("/go/build/", "Go build cache"),
            ("/library/caches/go-build/", "Go build cache (macOS)"),
            ("/cargo/registry/", "Cargo registry (alt)"),
            ("/.composer/cache/", "Composer cache"),
            ("/.nuget/packages/", "NuGet packages"),
            ("/.pub-cache/", "Dart/Flutter pub cache"),
        ];

        for (pattern, desc) in DEV_CACHE_PATTERNS {
            if lower_path.contains(pattern) {
                return Some(Finding::new(
                    rec.path.clone(),
                    rec.size,
                    Category::DevCache,
                    format!("кэш разработчика: {}", desc),
                    Risk::Safe,
                    rec.mod_time,
                ));
            }
        }

        // Check by directory name patterns
        let name = Path::new(&rec.path)
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_lowercase();

        if name == "target" || name == "node_modules" || name == "dist" || name == "build" || name == ".next" || name == ".output" {
            // Only classify if it's a directory marker (we don't scan dirs, but parent paths)
            // This would need directory context - skip for files
        }

        None
    }

    /// Check for Xcode caches (DerivedData, Archives, iOS DeviceSupport)
    fn check_xcode_cache(&self, rec: &FileRecord) -> Option<Finding> {
        #[cfg(target_os = "macos")]
        {
            if !self.config.scan_ide_caches {
                return None;
            }
            let lower_path = rec.path.to_lowercase();

            const XCODE_CACHE_DIRS: &[&str] = &[
                "/library/developer/xcode/deriveddata/",
                "/library/developer/xcode/archives/",
                "/library/developer/xcode/ios devicesupport/",
                "/library/developer/xcode/macdevicesupport/",
                "/library/developer/xcode/watchdevicesupport/",
                "/library/developer/xcode/tvosdevicesupport/",
                "/library/caches/com.apple.dt.xcode/",
                "/library/logs/diagnosticreports/xcode",
            ];

            for dir in XCODE_CACHE_DIRS {
                if lower_path.contains(dir) {
                    return Some(Finding::new(
                        rec.path.clone(),
                        rec.size,
                        Category::XcodeCache,
                        format!("Xcode кэш: {}", dir.trim_matches('/')),
                        Risk::Safe,
                        rec.mod_time,
                    ));
                }
            }
        }
        None
    }

    /// Check for VS Code / Cursor caches
    fn check_vscode_cache(&self, rec: &FileRecord) -> Option<Finding> {
        if !self.config.scan_ide_caches {
            return None;
        }
        let lower_path = rec.path.to_lowercase();

        const VSCODE_CACHE_DIRS: &[&str] = &[
            // VS Code
            "/library/application support/code/cacheddata/",
            "/library/application support/code/logs/",
            "/library/application support/code/user/workspaceStorage/",
            "/.vscode-server/data/",
            "/.vscode/extensions/",
            // Cursor
            "/library/application support/cursor/cacheddata/",
            "/library/application support/cursor/logs/",
            "/library/application support/cursor/user/workspaceStorage/",
            // Windows
            "appdata/roaming/code/cacheddata/",
            "appdata/roaming/code/logs/",
            "appdata/roaming/code/user/workspacestorage/",
            "appdata/roaming/cursor/cacheddata/",
            "appdata/roaming/cursor/logs/",
            "appdata/roaming/cursor/user/workspacestorage/",
            // Linux
            "/.config/code/cacheddata/",
            "/.config/code/logs/",
            "/.config/cursor/cacheddata/",
            "/.config/cursor/logs/",
        ];

        for dir in VSCODE_CACHE_DIRS {
            if lower_path.contains(dir) {
                return Some(Finding::new(
                    rec.path.clone(),
                    rec.size,
                    Category::VSCodeCache,
                    format!("VS Code/Cursor кэш: {}", dir.trim_matches('/')),
                    Risk::Safe,
                    rec.mod_time,
                ));
            }
        }

        // JetBrains IDEs
        const JETBRAINS_DIRS: &[&str] = &[
            "/library/caches/jetbrains/",
            "/library/logs/jetbrains/",
            "appdata/local/jetbrains/",
            "appdata/roaming/jetbrains/",
            "/.cache/jetbrains/",
            "/.local/share/jetbrains/",
        ];

        for dir in JETBRAINS_DIRS {
            if lower_path.contains(dir) {
                return Some(Finding::new(
                    rec.path.clone(),
                    rec.size,
                    Category::VSCodeCache,
                    format!("JetBrains IDE кэш: {}", dir.trim_matches('/')),
                    Risk::Safe,
                    rec.mod_time,
                ));
            }
        }

        None
    }

    /// Check for mail attachments
    fn check_mail_attachment(&self, rec: &FileRecord) -> Option<Finding> {
        if !self.config.scan_mail_attachments {
            return None;
        }
        let lower_path = rec.path.to_lowercase();
        let name = Path::new(&rec.path)
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("");
        let lower_name = name.to_lowercase();

        // Apple Mail
        #[cfg(target_os = "macos")]
        {
            const MAIL_DIRS: &[&str] = &[
                "/library/mail/",
                "/library/containers/com.apple.mail/",
                "/library/group containers/*.mail/",
            ];
            for dir in MAIL_DIRS {
                if lower_path.contains(dir) && lower_name.contains("attachment") {
                    return Some(Finding::new(
                        rec.path.clone(),
                        rec.size,
                        Category::MailAttachment,
                        "вложение Apple Mail".to_string(),
                        Risk::Safe,
                        rec.mod_time,
                    ));
                }
            }
        }

        // Windows Mail / Outlook
        #[cfg(target_os = "windows")]
        {
            const MAIL_DIRS: &[&str] = &[
                "appdata/local/microsoft/outlook/",
                "appdata/local/microsoft/windows mail/",
                "appdata/local/packages/microsoft.windowscommunicationsapps/",
            ];
            for dir in MAIL_DIRS {
                if lower_path.contains(dir) && (lower_name.contains("attachment") || lower_name.ends_with(".eml") || lower_name.ends_with(".msg")) {
                    return Some(Finding::new(
                        rec.path.clone(),
                        rec.size,
                        Category::MailAttachment,
                        "вложение почты Windows/Outlook".to_string(),
                        Risk::Safe,
                        rec.mod_time,
                    ));
                }
            }
        }

        None
    }

    /// Check for old backups (iOS, Time Machine, Windows Backup)
    fn check_old_backup(&self, rec: &FileRecord) -> Option<Finding> {
        if !self.config.scan_old_backups {
            return None;
        }
        let lower_path = rec.path.to_lowercase();
        let cutoff = chrono::Utc::now() - chrono::Duration::days(self.config.stale_install_days * 2); // Use longer period for backups

        // iOS backups
        #[cfg(target_os = "macos")]
        {
            if lower_path.contains("/library/application support/mobilesync/backup/") && rec.mod_time < cutoff.into() {
                return Some(Finding::new(
                    rec.path.clone(),
                    rec.size,
                    Category::OldBackup,
                    "старая резервная копия iOS".to_string(),
                    Risk::Caution,
                    rec.mod_time,
                ));
            }
            if lower_path.contains("/library/application support/mobilesync/") && lower_path.ends_with(".mddata") && rec.mod_time < cutoff.into() {
                return Some(Finding::new(
                    rec.path.clone(),
                    rec.size,
                    Category::OldBackup,
                    "файл резервной копии iOS".to_string(),
                    Risk::Caution,
                    rec.mod_time,
                ));
            }
        }

        #[cfg(target_os = "windows")]
        {
            if lower_path.contains("appdata/roaming/apple computer/mobilesync/backup/") && rec.mod_time < cutoff.into() {
                return Some(Finding::new(
                    rec.path.clone(),
                    rec.size,
                    Category::OldBackup,
                    "старая резервная копия iOS (Windows)".to_string(),
                    Risk::Caution,
                    rec.mod_time,
                ));
            }
            if lower_path.contains("appdata/local/microsoft/windows/backup/") && rec.mod_time < cutoff.into() {
                return Some(Finding::new(
                    rec.path.clone(),
                    rec.size,
                    Category::OldBackup,
                    "Windows Backup".to_string(),
                    Risk::Caution,
                    rec.mod_time,
                ));
            }
        }

        // Time Machine sparse bundles
        #[cfg(target_os = "macos")]
        {
            if lower_path.contains(".sparsebundle") || lower_path.contains(".backupbundle") {
                if rec.mod_time < cutoff.into() {
                    return Some(Finding::new(
                        rec.path.clone(),
                        rec.size,
                        Category::OldBackup,
                        "Time Machine образ".to_string(),
                        Risk::Caution,
                        rec.mod_time,
                    ));
                }
            }
        }

        None
    }

    /// Check for unused disk images (.dmg, .iso, .img, .vhd, .vhdx)
    fn check_unused_disk_image(&self, rec: &FileRecord) -> Option<Finding> {
        if !self.config.scan_unused_disk_images {
            return None;
        }
        let name = Path::new(&rec.path)
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("");
        let lower_name = name.to_lowercase();
        let cutoff = chrono::Utc::now() - chrono::Duration::days(self.config.unused_disk_image_days);

        const DISK_IMAGE_EXTS: &[&str] = &[
            ".dmg", ".iso", ".img", ".vhd", ".vhdx", ".vmdk", ".qcow2", ".toast", ".cdr",
        ];

        for ext in DISK_IMAGE_EXTS {
            if lower_name.ends_with(ext) && rec.mod_time < cutoff.into() && rec.size >= self.config.large_hidden_bytes as i64 {
                return Some(Finding::new(
                    rec.path.clone(),
                    rec.size,
                    Category::UnusedDiskImage,
                    format!("неиспользуемый образ диска ({})", ext),
                    Risk::Caution,
                    rec.mod_time,
                ));
            }
        }

        None
    }

    /// Check for unused language files (.lproj on macOS, MUI on Windows)
    fn check_language_file(&self, rec: &FileRecord) -> Option<Finding> {
        if !self.config.scan_language_files {
            return None;
        }
        let lower_path = rec.path.to_lowercase();

        // macOS .lproj bundles
        #[cfg(target_os = "macos")]
        {
            if lower_path.contains(".lproj/") || lower_path.ends_with(".lproj") {
                // Check if it's a non-system language (not English)
                let name = Path::new(&rec.path)
                    .file_name()
                    .and_then(|s| s.to_str())
                    .unwrap_or("")
                    .to_lowercase();
                if !name.starts_with("en") && !name.starts_with("base") {
                    return Some(Finding::new(
                        rec.path.clone(),
                        rec.size,
                        Category::LanguageFile,
                        "неиспользуемая локализация (.lproj)".to_string(),
                        Risk::Safe,
                        rec.mod_time,
                    ));
                }
            }
        }

        // Windows MUI files
        #[cfg(target_os = "windows")]
        {
            if lower_path.contains("\\mui\\") || lower_path.ends_with(".mui") {
                return Some(Finding::new(
                    rec.path.clone(),
                    rec.size,
                    Category::LanguageFile,
                    "файл локализации Windows (MUI)".to_string(),
                    Risk::Safe,
                    rec.mod_time,
                ));
            }
        }

        // Linux locale files
        #[cfg(all(unix, not(target_os = "macos")))]
        {
            if lower_path.contains("/usr/share/locale/") || lower_path.contains("/locale/") {
                let name = Path::new(&rec.path)
                    .file_name()
                    .and_then(|s| s.to_str())
                    .unwrap_or("")
                    .to_lowercase();
                if name.ends_with(".mo") || name.ends_with(".po") {
                    return Some(Finding::new(
                        rec.path.clone(),
                        rec.size,
                        Category::LanguageFile,
                        "файл локализации Linux".to_string(),
                        Risk::Safe,
                        rec.mod_time,
                    ));
                }
            }
        }

        None
    }

    /// Check for large hidden files
    fn check_large_hidden(&self, rec: &FileRecord) -> Option<Finding> {
        if !self.config.scan_large_hidden {
            return None;
        }
        let _name = Path::new(&rec.path)
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("");

        // Hidden file (starts with . on Unix, or hidden attribute on Windows)
        if rec.attrs.is_hidden && rec.size >= self.config.large_hidden_bytes as i64 {
            return Some(Finding::new(
                rec.path.clone(),
                rec.size,
                Category::LargeHidden,
                format!("большой скрытый файл (> {})", format_bytes(self.config.large_hidden_bytes)),
                Risk::Caution,
                rec.mod_time,
            ));
        }

        None
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
