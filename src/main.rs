//! unused-removal - Fast cross-platform file scanner and cleaner

mod config;
mod scanner;
mod scanner_types;
mod cache;
mod rules;
mod cleaner;
mod server;
mod cli;
mod tui;
mod desktop;

// Re-export scanner types for public API
pub use scanner_types::{
    Fingerprint, Attrs, FileRecord, ScanError, Options, ProgressSnapshot, DirId,
    FLUSH_BATCH, RECENT_CAP,
};
pub use crate::scanner::platform::Progress;

use anyhow::Result;
use clap::{Parser, Subcommand};
use tracing_subscriber::{EnvFilter, fmt};

use crate::config::{Config, SafetyLevel};
use crate::server::run_server;
use crate::cli::{scan_cmd, bench_cmd, config_cmd, smart_clean_cmd};

#[derive(Parser)]
#[command(name = "unused-removal", version, about = "Fast file scanner and cleaner for Windows", disable_version_flag = true)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,

    /// Path to config.toml
    #[arg(short, long, global = true)]
    config: Option<String>,

    /// Show version
    #[arg(short, long, global = true)]
    version: bool,
}

#[derive(Subcommand)]
enum Commands {
    /// Scan a directory (default action)
    Scan {
        /// Root path to scan
        #[arg(short, long)]
        root: Option<String>,
        /// Output JSON report
        #[arg(long)]
        json: Option<String>,
        /// Output CSV report
        #[arg(long)]
        csv: Option<String>,
        /// Disable cache
        #[arg(long)]
        no_cache: bool,
        /// Number of workers (0 = auto)
        #[arg(short, long)]
        workers: Option<usize>,
        /// Follow junction/symlink
        #[arg(long)]
        follow_links: bool,
        /// Find duplicates
        #[arg(long)]
        duplicates: bool,
        /// Protect system paths
        #[arg(long, default_value_t = true)]
        protect: bool,
        /// Show top N findings
        #[arg(short, long, default_value_t = 10)]
        top: usize,
    },
    /// Start web interface
    Serve {
        /// Port to listen on (0 = auto)
        #[arg(short, long)]
        port: Option<u16>,
    },
    /// Launch as a desktop application (native window; requires the `desktop` feature)
    #[cfg(feature = "desktop")]
    App {
        /// Port for the embedded UI server
        #[arg(short, long)]
        port: Option<u16>,
    },
    /// Run interactive TUI
    Tui,
    /// Run benchmark
    Bench {
        /// Number of test files
        #[arg(long, default_value_t = 100000)]
        files: usize,
        /// Directory depth
        #[arg(long, default_value_t = 4)]
        depth: usize,
        /// Run serial comparison
        #[arg(long)]
        serial: bool,
    },
    /// Show current configuration
    Config,
    /// Smart junk cleanup (one-click cleanup like CleanMyMac)
    SmartClean {
        /// Root path to scan (default: config root)
        #[arg(short, long)]
        root: Option<String>,
        /// Dry run - show what would be deleted without deleting
        #[arg(long)]
        dry_run: bool,
        /// Safety level: safe, balanced, aggressive
        #[arg(long, default_value = "balanced")]
        safety: String,
        /// Auto-confirm deletion (skip prompt)
        #[arg(long)]
        yes: bool,
        /// Output JSON report
        #[arg(long)]
        json: Option<String>,
        /// Output CSV report
        #[arg(long)]
        csv: Option<String>,
    },
}

fn main() -> Result<()> {
    // Initialize logging
    fmt()
        .with_env_filter(EnvFilter::from_default_env().add_directive("unused_removal=info".parse()?))
        .with_target(false)
        .init();

    let cli = Cli::parse();

    if cli.version {
        println!("unused-removal {}", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }

    // Load configuration
    let config = Config::load(cli.config.as_deref())?;

    match cli.command {
        Some(Commands::Scan { root, json, csv, no_cache, workers, follow_links, duplicates, protect, top }) => {
            let mut cfg = config;
            if let Some(r) = root { cfg.root = r; }
            if let Some(w) = workers { cfg.workers = w; }
            cfg.follow_links = follow_links;
            cfg.check_duplicates = duplicates;
            cfg.protect_system = protect;
            if no_cache { cfg.use_cache = false; }
            scan_cmd(&cfg, json, csv, top)?;
        }
        Some(Commands::Serve { port }) => {
            let mut cfg = config;
            if let Some(p) = port { cfg.web_port = p; }
            let rt = tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()?;
            rt.block_on(run_server(cfg))?;
        }
        #[cfg(feature = "desktop")]
        Some(Commands::App { port }) => {
            desktop::run_app(config, port)?;
        }
        Some(Commands::Tui) => {
            tui::run(config)?;
        }
        Some(Commands::Bench { files, depth, serial }) => {
            bench_cmd(&config, files, depth, serial)?;
        }
        Some(Commands::Config) => {
            config_cmd(&config)?;
        }
        Some(Commands::SmartClean { root, dry_run, safety, yes, json, csv }) => {
            let mut cfg = config;
            if let Some(r) = root { cfg.root = r; }
            // Parse safety level
            cfg.smart_junk_safety_level = match safety.to_lowercase().as_str() {
                "safe" => SafetyLevel::Safe,
                "balanced" => SafetyLevel::Balanced,
                "aggressive" => SafetyLevel::Aggressive,
                _ => SafetyLevel::Balanced,
            };
            smart_clean_cmd(&cfg, dry_run, yes, json, csv)?;
        }
        None => {
            // No subcommand: auto-detect terminal for TUI vs Web
            if atty::is(atty::Stream::Stdin) && atty::is(atty::Stream::Stdout) {
                tui::run(config)?;
            } else {
                let rt = tokio::runtime::Builder::new_multi_thread()
                    .enable_all()
                    .build()?;
                rt.block_on(run_server(config))?;
            }
        }
    }

    Ok(())
}