//! Cross-platform configuration with platform-specific defaults

use anyhow::Result;
use dirs;
use serde::{Deserialize, Serialize};
use std::env;
use std::path::{Path, PathBuf};
use toml;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum SafetyLevel {
    Safe,
    Balanced,
    Aggressive,
}

impl Default for SafetyLevel {
    fn default() -> Self {
        SafetyLevel::Balanced
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct Config {
    // General
    pub root: String,
    pub workers: usize,
    pub follow_links: bool,
    pub exclude_dirs: Vec<String>,
    pub exclude_prefix: Vec<String>,

    // Rules
    pub large_bytes: u64,
    pub huge_bytes: u64,
    pub stale_days: i64,
    pub old_log_days: i64,
    pub stale_install_days: i64,
    pub junk_extensions: Vec<String>,
    pub junk_dirs: Vec<String>,
    pub check_duplicates: bool,

    // Smart Junk
    pub smart_junk_enabled: bool,
    pub scan_user_caches: bool,
    pub scan_system_logs: bool,
    pub scan_language_files: bool,
    pub scan_old_backups: bool,
    pub scan_mail_attachments: bool,
    pub scan_trash: bool,
    pub scan_old_downloads: bool,
    pub scan_unused_disk_images: bool,
    pub scan_dev_caches: bool,
    pub scan_ide_caches: bool,
    pub scan_large_hidden: bool,

    // Smart Junk Thresholds
    pub old_download_days: i64,
    pub unused_disk_image_days: i64,
    pub large_hidden_bytes: u64,
    pub min_cache_size_bytes: u64,

    // Safety
    pub protect_system: bool,
    pub allow_protected: bool,
    pub smart_junk_safety_level: SafetyLevel,

    // Cache
    pub use_cache: bool,
    pub cache_dir: String,

}

impl Default for Config {
    fn default() -> Self {
        Self::with_platform_defaults()
    }
}

impl Config {
    /// Create config with platform-specific defaults
    fn with_platform_defaults() -> Self {
        #[cfg(windows)]
        {
            Self {
                root: r"C:\".to_string(),
                workers: 0,
                follow_links: false,
                exclude_dirs: vec![
                    r"System Volume Information".to_string(),
                    r"Windows\WinSxS".to_string(),
                    r"Windows\SoftwareDistribution".to_string(),
                    r"ProgramData\Microsoft\Windows Defender".to_string(),
                ],
                exclude_prefix: vec![],
                large_bytes: 100 * 1024 * 1024, // 100 MB
                huge_bytes: 500 * 1024 * 1024,  // 500 MB
                stale_days: 180,
                old_log_days: 30,
                stale_install_days: 90,
                junk_extensions: vec![
                    ".tmp".to_string(),
                    ".temp".to_string(),
                    ".bak".to_string(),
                    ".old".to_string(),
                    ".dmp".to_string(),
                    ".chk".to_string(),
                    "~$*".to_string(),
                ],
                // NOTE: browser cache dirs are handled by dedicated UserCache
                // rules with higher precision — do not swallow them into Junk.
                junk_dirs: vec![
                    "%TEMP%".to_string(),
                    r"C:\Windows\Temp".to_string(),
                    r"C:\Windows\Prefetch".to_string(),
                ],
                check_duplicates: false,
                // Smart Junk
                smart_junk_enabled: true,
                scan_user_caches: true,
                scan_system_logs: true,
                scan_language_files: true,
                scan_old_backups: true,
                scan_mail_attachments: true,
                scan_trash: true,
                scan_old_downloads: true,
                scan_unused_disk_images: true,
                scan_dev_caches: true,
                scan_ide_caches: true,
                scan_large_hidden: true,
                // Smart Junk Thresholds
                old_download_days: 30,
                unused_disk_image_days: 60,
                large_hidden_bytes: 50 * 1024 * 1024,   // 50 MB
                min_cache_size_bytes: 10 * 1024 * 1024, // 10 MB
                // Safety
                protect_system: true,
                allow_protected: false,
                smart_junk_safety_level: SafetyLevel::Balanced,
                // Cache
                use_cache: true,
                cache_dir: "".to_string(),
            }
        }

        #[cfg(target_os = "macos")]
        {
            Self {
                root: "/".to_string(),
                workers: 0,
                follow_links: false,
                // Scan local and mounted volumes completely. Protected macOS locations are kept
                // visible only for large-file review and remain blocked from deletion by rules.
                exclude_dirs: vec![],
                exclude_prefix: vec!["/Network".to_string()],
                large_bytes: 100 * 1024 * 1024, // 100 MB
                huge_bytes: 500 * 1024 * 1024,  // 500 MB
                stale_days: 180,
                old_log_days: 30,
                stale_install_days: 90,
                junk_extensions: vec![
                    ".tmp".to_string(),
                    ".temp".to_string(),
                    ".bak".to_string(),
                    ".old".to_string(),
                    ".dmp".to_string(),
                    ".chk".to_string(),
                    ".DS_Store".to_string(),
                ],
                // NOTE: ~/Library/{Caches,Logs} are classified by the dedicated
                // UserCache/SystemLog rules — do not duplicate them as Junk dirs.
                junk_dirs: vec![
                    "/tmp".to_string(),
                    "/private/tmp".to_string(),
                    "/var/folders".to_string(),
                ],
                check_duplicates: false,
                // Smart Junk
                smart_junk_enabled: true,
                scan_user_caches: true,
                scan_system_logs: true,
                scan_language_files: true,
                scan_old_backups: true,
                scan_mail_attachments: true,
                scan_trash: true,
                scan_old_downloads: true,
                scan_unused_disk_images: true,
                scan_dev_caches: true,
                scan_ide_caches: true,
                scan_large_hidden: true,
                // Smart Junk Thresholds
                old_download_days: 30,
                unused_disk_image_days: 60,
                large_hidden_bytes: 50 * 1024 * 1024,   // 50 MB
                min_cache_size_bytes: 10 * 1024 * 1024, // 10 MB
                // Safety
                protect_system: true,
                allow_protected: false,
                smart_junk_safety_level: SafetyLevel::Balanced,
                // Cache
                use_cache: true,
                cache_dir: "".to_string(),
            }
        }

        #[cfg(all(unix, not(target_os = "macos")))]
        {
            Self {
                root: "/".to_string(),
                workers: 0,
                follow_links: false,
                exclude_dirs: vec![
                    "proc".to_string(),
                    "sys".to_string(),
                    "dev".to_string(),
                    "run".to_string(),
                    "tmp".to_string(),
                    "var/tmp".to_string(),
                ],
                exclude_prefix: vec![],
                large_bytes: 100 * 1024 * 1024,
                huge_bytes: 500 * 1024 * 1024,
                stale_days: 180,
                old_log_days: 30,
                stale_install_days: 90,
                junk_extensions: vec![
                    ".tmp".to_string(),
                    ".temp".to_string(),
                    ".bak".to_string(),
                    ".old".to_string(),
                    ".dmp".to_string(),
                    ".chk".to_string(),
                ],
                junk_dirs: vec![
                    "/tmp".to_string(),
                    "/var/tmp".to_string(),
                    "$HOME/.cache".to_string(),
                    "$HOME/.local/share/Trash".to_string(),
                ],
                check_duplicates: false,
                // Smart Junk
                smart_junk_enabled: true,
                scan_user_caches: true,
                scan_system_logs: true,
                scan_language_files: true,
                scan_old_backups: true,
                scan_mail_attachments: true,
                scan_trash: true,
                scan_old_downloads: true,
                scan_unused_disk_images: true,
                scan_dev_caches: true,
                scan_ide_caches: true,
                scan_large_hidden: true,
                // Smart Junk Thresholds
                old_download_days: 30,
                unused_disk_image_days: 60,
                large_hidden_bytes: 50 * 1024 * 1024,   // 50 MB
                min_cache_size_bytes: 10 * 1024 * 1024, // 10 MB
                // Safety
                protect_system: true,
                allow_protected: false,
                smart_junk_safety_level: SafetyLevel::Balanced,
                // Cache
                use_cache: true,
                cache_dir: "".to_string(),
            }
        }
    }

    pub fn load(explicit_path: Option<&str>) -> Result<Self> {
        let mut cfg = Self::default();

        let mut paths = Vec::new();
        if let Some(p) = explicit_path {
            paths.push(PathBuf::from(p));
        }
        paths.push(PathBuf::from("config.toml"));
        if let Some(local) = dirs::data_local_dir() {
            paths.push(local.join("unused-removal").join("config.toml"));
        }

        for p in paths {
            if p.exists() {
                let content = std::fs::read_to_string(&p)?;
                let decoded: Config = toml::from_str(&content)?;
                // Merge with defaults (explicit values override)
                cfg.merge(&decoded);
                break;
            }
        }

        // Expand environment variables in paths
        cfg.expand_env_vars();

        // Validate and apply defaults
        cfg.validate();

        Ok(cfg)
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        let content = toml::to_string_pretty(self)?;
        std::fs::write(path, content)?;
        Ok(())
    }

    fn merge(&mut self, other: &Config) {
        // Only override non-default values
        macro_rules! merge_field {
            ($field:ident, $default:expr) => {
                if other.$field != $default {
                    self.$field = other.$field.clone();
                }
            };
        }

        merge_field!(root, Self::default().root);
        merge_field!(workers, Self::default().workers);
        merge_field!(follow_links, Self::default().follow_links);
        merge_field!(exclude_dirs, Self::default().exclude_dirs);
        merge_field!(exclude_prefix, Self::default().exclude_prefix);
        merge_field!(large_bytes, Self::default().large_bytes);
        merge_field!(huge_bytes, Self::default().huge_bytes);
        merge_field!(stale_days, Self::default().stale_days);
        merge_field!(old_log_days, Self::default().old_log_days);
        merge_field!(stale_install_days, Self::default().stale_install_days);
        merge_field!(junk_extensions, Self::default().junk_extensions);
        merge_field!(junk_dirs, Self::default().junk_dirs);
        merge_field!(check_duplicates, Self::default().check_duplicates);
        // Smart Junk
        merge_field!(smart_junk_enabled, Self::default().smart_junk_enabled);
        merge_field!(scan_user_caches, Self::default().scan_user_caches);
        merge_field!(scan_system_logs, Self::default().scan_system_logs);
        merge_field!(scan_language_files, Self::default().scan_language_files);
        merge_field!(scan_old_backups, Self::default().scan_old_backups);
        merge_field!(scan_mail_attachments, Self::default().scan_mail_attachments);
        merge_field!(scan_trash, Self::default().scan_trash);
        merge_field!(scan_old_downloads, Self::default().scan_old_downloads);
        merge_field!(
            scan_unused_disk_images,
            Self::default().scan_unused_disk_images
        );
        merge_field!(scan_dev_caches, Self::default().scan_dev_caches);
        merge_field!(scan_ide_caches, Self::default().scan_ide_caches);
        merge_field!(scan_large_hidden, Self::default().scan_large_hidden);
        // Smart Junk Thresholds
        merge_field!(old_download_days, Self::default().old_download_days);
        merge_field!(
            unused_disk_image_days,
            Self::default().unused_disk_image_days
        );
        merge_field!(large_hidden_bytes, Self::default().large_hidden_bytes);
        merge_field!(min_cache_size_bytes, Self::default().min_cache_size_bytes);
        // Safety
        merge_field!(protect_system, Self::default().protect_system);
        merge_field!(allow_protected, Self::default().allow_protected);
        merge_field!(
            smart_junk_safety_level,
            Self::default().smart_junk_safety_level
        );
        // Cache
        merge_field!(use_cache, Self::default().use_cache);
        merge_field!(cache_dir, Self::default().cache_dir);
    }

    fn expand_env_vars(&mut self) {
        fn expand_vars(s: &str) -> String {
            let mut result = String::with_capacity(s.len());
            let mut chars = s.chars().peekable();

            while let Some(ch) = chars.next() {
                if ch == '%' {
                    // Windows %VAR% syntax
                    let mut var_name = String::new();
                    let mut closed = false;
                    while let Some(&c) = chars.peek() {
                        chars.next();
                        if c == '%' {
                            closed = true;
                            break;
                        }
                        var_name.push(c);
                    }
                    if closed && !var_name.is_empty() {
                        if let Ok(val) = env::var(&var_name) {
                            result.push_str(&val);
                        } else if var_name == "TEMP" || var_name == "TMP" {
                            result.push_str(&env::temp_dir().to_string_lossy());
                        } else {
                            result.push('%');
                            result.push_str(&var_name);
                            result.push('%');
                        }
                    } else {
                        result.push('%');
                        result.push_str(&var_name);
                    }
                } else if ch == '$' {
                    // Unix $VAR or ${VAR} syntax
                    let mut var_name = String::new();
                    if chars.peek() == Some(&'{') {
                        chars.next(); // consume '{'
                        while let Some(&c) = chars.peek() {
                            chars.next();
                            if c == '}' {
                                break;
                            }
                            var_name.push(c);
                        }
                    } else {
                        while let Some(&c) = chars.peek() {
                            if c.is_alphanumeric() || c == '_' {
                                var_name.push(c);
                                chars.next();
                            } else {
                                break;
                            }
                        }
                    }
                    if !var_name.is_empty() {
                        if let Ok(val) = env::var(&var_name) {
                            result.push_str(&val);
                        } else if var_name == "HOME" {
                            if let Some(h) = dirs::home_dir() {
                                result.push_str(&h.to_string_lossy());
                            } else {
                                result.push('$');
                                result.push_str(&var_name);
                            }
                        } else if var_name == "TMPDIR" {
                            result.push_str(&env::temp_dir().to_string_lossy());
                        } else {
                            result.push('$');
                            result.push_str(&var_name);
                        }
                    } else {
                        result.push('$');
                    }
                } else {
                    result.push(ch);
                }
            }
            result
        }

        self.exclude_dirs = self.exclude_dirs.iter().map(|s| expand_vars(s)).collect();
        self.exclude_prefix = self.exclude_prefix.iter().map(|s| expand_vars(s)).collect();
        self.junk_dirs = self.junk_dirs.iter().map(|s| expand_vars(s)).collect();
    }

    fn validate(&mut self) {
        if self.workers == 0 {
            self.workers = num_cpus::get();
        }
        if self.large_bytes == 0 {
            self.large_bytes = 100 * 1024 * 1024;
        }
        if self.huge_bytes == 0 {
            self.huge_bytes = 500 * 1024 * 1024;
        }
        if self.stale_days <= 0 {
            self.stale_days = 180;
        }
        if self.old_log_days <= 0 {
            self.old_log_days = 30;
        }
        if self.stale_install_days <= 0 {
            self.stale_install_days = 90;
        }
        // Smart Junk Thresholds
        if self.old_download_days <= 0 {
            self.old_download_days = 30;
        }
        if self.unused_disk_image_days <= 0 {
            self.unused_disk_image_days = 60;
        }
        if self.large_hidden_bytes == 0 {
            self.large_hidden_bytes = 50 * 1024 * 1024;
        }
        if self.min_cache_size_bytes == 0 {
            self.min_cache_size_bytes = 10 * 1024 * 1024;
        }
    }

    pub fn stale_cutoff(&self) -> chrono::DateTime<chrono::Utc> {
        chrono::Utc::now() - chrono::Duration::days(self.stale_days)
    }

    pub fn old_log_cutoff(&self) -> chrono::DateTime<chrono::Utc> {
        chrono::Utc::now() - chrono::Duration::days(self.old_log_days)
    }

    pub fn stale_install_cutoff(&self) -> chrono::DateTime<chrono::Utc> {
        chrono::Utc::now() - chrono::Duration::days(self.stale_install_days)
    }

    /// Minimum cache size in bytes for detection
    pub fn min_cache_size_bytes(&self) -> u64 {
        self.min_cache_size_bytes
    }

    /// Old download cutoff date
    pub fn old_download_cutoff(&self) -> chrono::DateTime<chrono::Utc> {
        chrono::Utc::now() - chrono::Duration::days(self.old_download_days)
    }

    /// Unused disk image cutoff date
    pub fn unused_disk_image_cutoff(&self) -> chrono::DateTime<chrono::Utc> {
        chrono::Utc::now() - chrono::Duration::days(self.unused_disk_image_days)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_expand_percent_vars() {
        let mut cfg = Config::default();
        cfg.junk_dirs = vec!["%TEMP%".to_string()];
        cfg.expand_env_vars();
        assert!(!cfg.junk_dirs[0].contains('%'));
    }

    #[test]
    fn test_expand_dollar_vars() {
        let mut cfg = Config::default();
        cfg.junk_dirs = vec!["$HOME/.cache".to_string()];
        cfg.expand_env_vars();
        assert!(!cfg.junk_dirs[0].contains('$'));
    }
}
