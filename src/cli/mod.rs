//! CLI command implementations

use std::sync::Arc;
use std::time::Instant;
use std::path::Path;
use anyhow::Result;
use indicatif::{ProgressBar, ProgressStyle, MultiProgress};
use colored::*;
use tempfile;

use crate::config::Config;
use crate::scanner::{Scanner, Progress as ScannerProgress};
use crate::scanner_types::Options;
use crate::cache::{Cache, BoltCache, config_hash as cache_config_hash};
use crate::rules::{Engine, Finding, Category};

/// Run scan command
pub fn scan_cmd(
    config: &Config,
    json_out: Option<String>,
    csv_out: Option<String>,
    top: usize,
) -> Result<()> {
    println!("{}", "Scanning...".bold().cyan());
    println!("  Root: {}", config.root.yellow());
    println!("  Workers: {}", config.workers.to_string().yellow());

    let start = Instant::now();

    // Progress bar
    let multi = MultiProgress::new();
    let pb = multi.add(ProgressBar::new_spinner());
    pb.set_style(
        ProgressStyle::default_spinner()
            .template("{spinner:.green} {msg}")
            .unwrap()
    );
    pb.enable_steady_tick(std::time::Duration::from_millis(100));

    let progress = ScannerProgress::new();
    let progress_clone = progress.clone();

    // Spawn progress updater
    let pb_for_thread = pb.clone();
    let progress_handle = std::thread::spawn(move || {
        loop {
            std::thread::sleep(std::time::Duration::from_millis(200));
            let snap = progress_clone.snapshot();
            if snap.finished { break; }
            let msg = format!(
                "{} files, {} · {:.0} files/s",
                format_number(snap.files),
                format_bytes(snap.bytes as u64),
                snap.rate_fps
            );
            if snap.cached > 0 {
                pb_for_thread.set_message(format!("{} · {} from cache", msg, format_number(snap.cached)));
            } else {
                pb_for_thread.set_message(msg);
            }
        }
    });

    // Scanner options
    let opts = Options {
        workers: config.workers,
        follow_links: config.follow_links,
        exclude: config.exclude_dirs.clone(),
        exclude_pref: config.exclude_prefix.clone(),
    };

    // Cache
    let cache: Option<Arc<dyn Cache>> = if config.use_cache {
        let hash = cache_config_hash(&opts);
        BoltCache::new("unused-removal", &hash).ok().map(|c| Arc::new(c) as Arc<dyn Cache>)
    } else {
        None
    };

    let scanner = Scanner::new(opts, progress, cache);
    
    // Run scan in blocking task
    let (records, errors) = scanner.walk(&config.root)?;
    
    pb.finish_and_clear();
    progress_handle.join().unwrap();

    println!("\n{}", "Analyzing...".bold().cyan());
    
    // Rules engine
    let engine = Engine::new(std::sync::Arc::new(config.clone()));
    let mut findings = engine.analyze(&records);
    
    if config.check_duplicates {
        let dups = engine.find_duplicates(&records);
        findings.extend(dups);
    }
    
    // Sort by size descending
    findings.sort_by(|a, b| b.size.cmp(&a.size));

    let elapsed = start.elapsed().as_secs_f64();
    println!(
        "\n{} {:.2}s. Files: {}, Findings: {}, Errors: {}",
        "Done in".green(),
        elapsed,
        format_number(records.len() as i64),
        findings.len(),
        errors.len()
    );

    // Print summary
    print_summary(&findings);
    
    if top > 0 {
        print_top(&findings, top);
    }

    // Export
    if let Some(path) = json_out {
        write_json(&path, &findings)?;
        println!("\n{} JSON report saved to {}", "✓".green(), path);
    }
    if let Some(path) = csv_out {
        write_csv(&path, &findings)?;
        println!("{} CSV report saved to {}", "✓".green(), path);
    }

    Ok(())
}

/// Run benchmark
pub fn bench_cmd(config: &Config, files: usize, depth: usize, serial: bool) -> Result<()> {
    println!("{}", format!("Benchmark: {} files, depth {}", files, depth).bold().cyan());

    // Create test fixture
    let tmp_dir = tempfile::tempdir()?;
    println!("Creating fixture...");
    let fixture_start = Instant::now();
    create_fixture(tmp_dir.path(), files, depth)?;
    println!("Fixture created in {:.2}s", fixture_start.elapsed().as_secs_f64());

    let mut cfg = config.clone();
    cfg.root = tmp_dir.path().to_string_lossy().to_string();
    cfg.workers = num_cpus::get();
    cfg.use_cache = false;

    let opts = Options {
        workers: cfg.workers,
        follow_links: cfg.follow_links,
        exclude: cfg.exclude_dirs.clone(),
        exclude_pref: cfg.exclude_prefix.clone(),
    };

    // Parallel scan
    let progress = ScannerProgress::new();
    let scanner = Scanner::new(opts.clone(), progress, None);
    
    let start = Instant::now();
    let (records, _) = scanner.walk(&cfg.root)?;
    let par_time = start.elapsed();
    
    println!(
        "Parallel: {:.2}s, {} files, {:.0} files/s",
        par_time.as_secs_f64(),
        records.len(),
        records.len() as f64 / par_time.as_secs_f64()
    );

    // Serial scan for comparison
    if serial {
        cfg.workers = 1;
        let opts_serial = Options { workers: 1, ..opts };
        let progress2 = ScannerProgress::new();
        let scanner2 = Scanner::new(opts_serial, progress2, None);
        
        let start = Instant::now();
        let (records2, _) = scanner2.walk(&cfg.root)?;
        let ser_time = start.elapsed();
        
        println!(
            "Serial: {:.2}s, {} files, {:.0} files/s",
            ser_time.as_secs_f64(),
            records2.len(),
            records2.len() as f64 / ser_time.as_secs_f64()
        );
        
        if ser_time > par_time {
            let speedup = ser_time.as_secs_f64() / par_time.as_secs_f64();
            println!("Speedup: {:.2}x", speedup);
        }
    }

    Ok(())
}

/// Show configuration
pub fn config_cmd(config: &Config) -> Result<()> {
    let json = serde_json::to_string_pretty(config)?;
    println!("{}", json);
    Ok(())
}

fn print_summary(findings: &[Finding]) {
    use std::collections::HashMap;
    
    let mut by_cat: HashMap<Category, (usize, u64)> = HashMap::new();
    for f in findings {
        let entry = by_cat.entry(f.category).or_insert((0, 0));
        entry.0 += 1;
        entry.1 += f.size as u64;
    }

    println!("\n{}", "Summary by category:".bold());
    println!("{:<20} {:>8} {:>12}", "Category", "Count", "Size");
    println!("{}", "─".repeat(42));
    
    let order = [
        Category::Huge,
        Category::Large,
        Category::Junk,
        Category::OldLog,
        Category::StaleInstall,
        Category::Stale,
        Category::Duplicate,
    ];
    
    for cat in order {
        if let Some((count, size)) = by_cat.get(&cat) {
            let icon = category_icon(cat);
            println!("{} {:<18} {:>8} {:>12}", icon, format!("{:?}", cat), count, format_bytes(*size));
        }
    }
}

fn print_top(findings: &[Finding], n: usize) {
    println!("\n{}", format!("Top {} largest:", n.min(findings.len())).bold());
    for (i, f) in findings.iter().take(n).enumerate() {
        let icon = category_icon(f.category);
        let risk_color = match f.risk {
            crate::rules::Risk::Safe => "green",
            crate::rules::Risk::Caution => "yellow",
            crate::rules::Risk::Protected => "red",
        };
        println!(
            "  {}. {} {:>10}  {}",
            i + 1,
            icon,
            format_bytes(f.size as u64).color(risk_color),
            f.path.dimmed()
        );
    }
}

fn category_icon(cat: Category) -> &'static str {
    match cat {
        Category::Huge => "🔴",
        Category::Large => "🟠",
        Category::Junk => "🗑",
        Category::OldLog => "📄",
        Category::StaleInstall => "📦",
        Category::Stale => "⏳",
        Category::Duplicate => "🔁",
        Category::AppLeftovers => "📦",
        Category::UserCache => "💾",
        Category::SystemLog => "📋",
        Category::LanguageFile => "🌐",
        Category::OldBackup => "💿",
        Category::MailAttachment => "📎",
        Category::Trash => "🗑️",
        Category::OldDownload => "⬇️",
        Category::UnusedDiskImage => "💿",
        Category::DevCache => "⚙️",
        Category::XcodeCache => "🛠️",
        Category::VSCodeCache => "💻",
        Category::LargeHidden => "🔍",
    }
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
    format!("{:.1} {}iB", bytes as f64 / div as f64, "KMGTPE".chars().nth(exp).unwrap())
}

fn format_number(n: i64) -> String {
    let s = n.to_string();
    let mut result = String::new();
    for (i, ch) in s.chars().rev().enumerate() {
        if i > 0 && i % 3 == 0 {
            result.push(' ');
        }
        result.push(ch);
    }
    result.chars().rev().collect()
}

fn write_json(path: &str, findings: &[Finding]) -> Result<()> {
    let json = serde_json::to_string_pretty(findings)?;
    std::fs::write(path, json)?;
    Ok(())
}

fn write_csv(path: &str, findings: &[Finding]) -> Result<()> {
    let mut csv = String::new();
    csv.push_str("path,size_bytes,category,reason,risk,mod_time\n");
    for f in findings {
        let mod_time: chrono::DateTime<chrono::Utc> = f.mod_time.into();
        csv.push_str(&format!(
            "{},{},{},{},{},{}\n",
            escape_csv(&f.path),
            f.size,
            f.category,
            escape_csv(&f.reason),
            format!("{:?}", f.risk),
            mod_time.to_rfc3339()
        ));
    }
    std::fs::write(path, csv)?;
    Ok(())
}

fn escape_csv(s: &str) -> String {
    if s.contains(',') || s.contains('"') || s.contains('\n') {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

/// Create test fixture for benchmarking — exactly `total_files` files,
/// spread across a shallow directory tree so fixture creation is fast.
fn create_fixture(root: &Path, total_files: usize, depth: usize) -> Result<()> {
    use std::sync::atomic::{AtomicUsize, Ordering};

    let per_dir = (total_files / 64).max(1);
    let counter = AtomicUsize::new(0);

    fn create_dir_recursive(
        path: &Path,
        current_depth: usize,
        max_depth: usize,
        per_dir: usize,
        total: usize,
        counter: &AtomicUsize,
    ) -> Result<()> {
        if current_depth >= max_depth || counter.load(Ordering::Relaxed) >= total {
            return Ok(());
        }
        for i in 0..10 {
            let subdir = path.join(format!("dir_{}_{}", current_depth, i));
            std::fs::create_dir_all(&subdir)?;
            for _ in 0..per_dir {
                if counter.fetch_add(1, Ordering::Relaxed) >= total {
                    return Ok(());
                }
                let file = subdir.join(format!("file_{}.tmp", counter.load(Ordering::Relaxed)));
                std::fs::write(&file, vec![b'x'; 1024])?;
            }
            create_dir_recursive(&subdir, current_depth + 1, max_depth, per_dir, total, counter)?;
        }
        Ok(())
    }

    let _ = depth;
    let total = total_files.max(1);
    create_dir_recursive(root, 0, depth.min(4), per_dir, total, &counter)?;
    Ok(())
}

/// Run smart clean command (one-click junk cleanup like CleanMyMac)
pub fn smart_clean_cmd(
    config: &Config,
    dry_run: bool,
    yes: bool,
    json_out: Option<String>,
    csv_out: Option<String>,
) -> Result<()> {
    if !config.smart_junk_enabled {
        println!("{}", "Smart junk detection is disabled in config".yellow());
        return Ok(());
    }

    println!("{}", "🧹 Smart Junk Cleanup".bold().cyan());
    println!("  Root: {}", config.root.yellow());
    println!("  Safety: {:?}", config.smart_junk_safety_level);
    println!("  Dry run: {}", if dry_run { "yes" } else { "no" }.to_string().yellow());

    let start = Instant::now();

    // Progress bar
    let multi = MultiProgress::new();
    let pb = multi.add(ProgressBar::new_spinner());
    pb.set_style(
        ProgressStyle::default_spinner()
            .template("{spinner:.green} {msg}")
            .unwrap()
    );
    pb.enable_steady_tick(std::time::Duration::from_millis(100));

    let progress = ScannerProgress::new();
    let progress_clone = progress.clone();

    // Spawn progress updater
    let pb_for_thread = pb.clone();
    let progress_handle = std::thread::spawn(move || {
        loop {
            std::thread::sleep(std::time::Duration::from_millis(200));
            let snap = progress_clone.snapshot();
            if snap.finished { break; }
            let msg = format!(
                "{} files, {} · {:.0} files/s",
                format_number(snap.files),
                format_bytes(snap.bytes as u64),
                snap.rate_fps
            );
            if snap.cached > 0 {
                pb_for_thread.set_message(format!("{} · {} from cache", msg, format_number(snap.cached)));
            } else {
                pb_for_thread.set_message(msg);
            }
        }
    });

    // Scanner options
    let opts = Options {
        workers: config.workers,
        follow_links: config.follow_links,
        exclude: config.exclude_dirs.clone(),
        exclude_pref: config.exclude_prefix.clone(),
    };

    // Cache
    let cache: Option<Arc<dyn Cache>> = if config.use_cache {
        let hash = cache_config_hash(&opts);
        BoltCache::new("unused-removal", &hash).ok().map(|c| Arc::new(c) as Arc<dyn Cache>)
    } else {
        None
    };

    let scanner = Scanner::new(opts, progress, cache);

    // Run scan in blocking task
    let (records, errors) = scanner.walk(&config.root)?;

    pb.finish_and_clear();
    progress_handle.join().unwrap();

    println!("\n{}", "Analyzing...".bold().cyan());

    // Rules engine
    let engine = Engine::new(std::sync::Arc::new(config.clone()));
    let mut findings = engine.analyze(&records);

    if config.check_duplicates {
        let dups = engine.find_duplicates(&records);
        findings.extend(dups);
    }

    // Sort by size descending
    findings.sort_by(|a, b| b.size.cmp(&a.size));

    let elapsed = start.elapsed().as_secs_f64();
    println!(
        "\n{} {:.2}s. Files: {}, Findings: {}, Errors: {}",
        "Done in".green(),
        elapsed,
        format_number(records.len() as i64),
        findings.len(),
        errors.len()
    );

    // Filter findings based on safety level
    let findings = filter_by_safety(findings, config);

    // Print summary by category
    print_smart_summary(&findings);

    if findings.is_empty() {
        println!("\n{}", "✨ No junk found to clean!".green());
        return Ok(());
    }

    // Calculate total reclaimable space
    let total_reclaimable: u64 = findings.iter().map(|f| f.size as u64).sum();
    println!("\n{}", format!("Total reclaimable: {}", format_bytes(total_reclaimable)).bold().green());

    if dry_run {
        println!("\n{}", "🔍 Dry run mode - no files were deleted".yellow());
    } else {
        // Confirm deletion
        if !yes {
            println!("\n{}", "⚠️  This will move files to Trash/Recycle Bin.".yellow());
            print!("Continue? [y/N]: ");
            use std::io::{stdin, stdout, Write};
            stdout().flush()?;
            let mut input = String::new();
            stdin().read_line(&mut input)?;
            if !input.trim().eq_ignore_ascii_case("y") {
                println!("{}", "Cancelled.".yellow());
                return Ok(());
            }
        }

        // Perform deletion
        println!("\n{}", "Deleting files...".bold().cyan());
        let paths: Vec<String> = findings.iter().map(|f| f.path.clone()).collect();
        let result = crate::cleaner::recycle_bin(&paths)?;

        println!(
            "{} Deleted: {}, Failed: {}, Freed: {}",
            "✓".green(),
            result.deleted.len(),
            result.failed.len(),
            format_bytes(result.total_bytes)
        );

        if !result.failed.is_empty() {
            println!("\n{}", "Failed deletions:".red());
            for err in &result.failed {
                println!("  {} - {}", err.path, err.error);
            }
        }
    }

    // Export
    if let Some(path) = json_out {
        write_json(&path, &findings)?;
        println!("\n{} JSON report saved to {}", "✓".green(), path);
    }
    if let Some(path) = csv_out {
        write_csv(&path, &findings)?;
        println!("{} CSV report saved to {}", "✓".green(), path);
    }

    Ok(())
}

/// Filter findings based on safety level
fn filter_by_safety(findings: Vec<Finding>, config: &Config) -> Vec<Finding> {
    use crate::config::SafetyLevel;
    use crate::rules::Category;

    let safety = config.smart_junk_safety_level;

    // Categories allowed per safety level
    let allowed_categories: Vec<Category> = match safety {
        SafetyLevel::Safe => vec![
            Category::Junk,
            Category::UserCache,
            Category::SystemLog,
            Category::Trash,
            Category::OldDownload,
            Category::DevCache,
            Category::XcodeCache,
            Category::VSCodeCache,
            Category::OldLog,
            Category::StaleInstall,
        ],
        SafetyLevel::Balanced => vec![
            Category::Junk,
            Category::UserCache,
            Category::SystemLog,
            Category::Trash,
            Category::OldDownload,
            Category::DevCache,
            Category::XcodeCache,
            Category::VSCodeCache,
            Category::OldLog,
            Category::StaleInstall,
            Category::LanguageFile,
            Category::OldBackup,
            Category::MailAttachment,
        ],
        SafetyLevel::Aggressive => vec![
            Category::Junk,
            Category::UserCache,
            Category::SystemLog,
            Category::Trash,
            Category::OldDownload,
            Category::DevCache,
            Category::XcodeCache,
            Category::VSCodeCache,
            Category::OldLog,
            Category::StaleInstall,
            Category::LanguageFile,
            Category::OldBackup,
            Category::MailAttachment,
            Category::UnusedDiskImage,
            Category::LargeHidden,
            Category::Stale,
            Category::Duplicate,
            Category::AppLeftovers,
        ],
    };

    findings.into_iter()
        .filter(|f| allowed_categories.contains(&f.category))
        .collect()
}

/// Print smart cleanup summary
fn print_smart_summary(findings: &[Finding]) {
    use std::collections::HashMap;
    use crate::rules::Category;

    let mut by_cat: HashMap<Category, (usize, u64)> = HashMap::new();
    for f in findings {
        let entry = by_cat.entry(f.category).or_insert((0, 0));
        entry.0 += 1;
        entry.1 += f.size as u64;
    }

    println!("\n{}", "Findings by category:".bold());
    println!("{:<25} {:>8} {:>12} {}", "Category", "Count", "Size", "Risk");
    println!("{}", "─".repeat(55));

    let order = [
        Category::UserCache,
        Category::SystemLog,
        Category::DevCache,
        Category::XcodeCache,
        Category::VSCodeCache,
        Category::Trash,
        Category::OldDownload,
        Category::Junk,
        Category::OldLog,
        Category::StaleInstall,
        Category::LanguageFile,
        Category::OldBackup,
        Category::MailAttachment,
        Category::UnusedDiskImage,
        Category::LargeHidden,
        Category::Stale,
        Category::Duplicate,
        Category::AppLeftovers,
        Category::Huge,
        Category::Large,
    ];

    for cat in order {
        if let Some((count, size)) = by_cat.get(&cat) {
            let icon = category_icon(cat);
            let risk_str = match cat {
                Category::Junk | Category::UserCache | Category::SystemLog | Category::Trash | Category::OldDownload | Category::DevCache | Category::XcodeCache | Category::VSCodeCache | Category::OldLog | Category::StaleInstall | Category::LanguageFile | Category::MailAttachment => "Safe",
                Category::OldBackup | Category::UnusedDiskImage | Category::LargeHidden | Category::Stale | Category::Duplicate | Category::AppLeftovers | Category::Huge | Category::Large => "Caution",
            };
            println!("{} {:<23} {:>8} {:>12} {}", icon, format!("{:?}", cat), count, format_bytes(*size), risk_str);
        }
    }
}