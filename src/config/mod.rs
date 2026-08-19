use std::path::{Path, PathBuf};
use std::env;
use serde::{Deserialize, Serialize};
use toml;
use anyhow::Result;
use dirs;

#[derive(Debug, Clone, Deserialize, Serialize)]
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

    // Safety
    pub protect_system: bool,
    pub allow_protected: bool,

    // Cache
    pub use_cache: bool,
    pub cache_dir: String,

    // Web
    pub web_port: u16,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            root: r"C:\".to_string(),
            workers: 0, // 0 = auto (num CPUs)
            follow_links: false,
            exclude_dirs: vec![
                r"$Recycle.Bin".to_string(),
                r"System Volume Information".to_string(),
                r"Windows\WinSxS".to_string(),
                r"Windows\SoftwareDistribution".to_string(),
                r"ProgramData\Microsoft\Windows Defender".to_string(),
            ],
            exclude_prefix: vec![],
            large_bytes: 100 * 1024 * 1024,      // 100 MB
            huge_bytes: 500 * 1024 * 1024,       // 500 MB
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
            junk_dirs: vec![
                "%TEMP%".to_string(),
                r"C:\Windows\Temp".to_string(),
                r"C:\Windows\Prefetch".to_string(),
                r"%LOCALAPPDATA%\Google\Chrome\User Data\Default\Cache".to_string(),
                r"%LOCALAPPDATA%\Microsoft\Edge\User Data\Default\Cache".to_string(),
                r"%LOCALAPPDATA%\Mozilla\Firefox\Profiles".to_string(),
            ],
            check_duplicates: false,
            protect_system: true,
            allow_protected: false,
            use_cache: true,
            cache_dir: "".to_string(),
            web_port: 0, // auto
        }
    }
}

impl Config {
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
                let mut decoded: Config = toml::from_str(&content)?;
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
        merge_field!(protect_system, Self::default().protect_system);
        merge_field!(allow_protected, Self::default().allow_protected);
        merge_field!(use_cache, Self::default().use_cache);
        merge_field!(cache_dir, Self::default().cache_dir);
        merge_field!(web_port, Self::default().web_port);
    }

    fn expand_env_vars(&mut self) {
        fn expand_percent_vars(s: &str) -> String {
            let mut result = String::with_capacity(s.len());
            let mut chars = s.chars().peekable();

            while let Some(ch) = chars.next() {
                if ch == '%' {
                    let mut var_name = String::new();
                    while let Some(&c) = chars.peek() {
                        if c == '%' {
                            chars.next(); // consume closing %
                            break;
                        }
                        var_name.push(chars.next().unwrap());
                    }
                    if let Ok(val) = env::var(&var_name) {
                        result.push_str(&val);
                    } else {
                        // Keep original %VAR% if not found
                        result.push('%');
                        result.push_str(&var_name);
                        result.push('%');
                    }
                } else {
                    result.push(ch);
                }
            }
            result
        }

        self.exclude_dirs = self.exclude_dirs.iter().map(|s| expand_percent_vars(s)).collect();
        self.exclude_prefix = self.exclude_prefix.iter().map(|s| expand_percent_vars(s)).collect();
        self.junk_dirs = self.junk_dirs.iter().map(|s| expand_percent_vars(s)).collect();
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
        if self.web_port == 0 {
            self.web_port = 8080;
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
}