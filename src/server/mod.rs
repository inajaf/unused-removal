//! HTTP server with embedded web UI and REST API

use std::collections::HashMap;
use std::net::SocketAddr;
use std::path::Path;
use std::str::FromStr;
use std::sync::{Arc, Mutex};

use axum::body::Body;
use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::{IntoResponse, Json, Response},
    routing::{get, post},
    Router,
};
use mime_guess::from_path;
use rust_embed::RustEmbed;
use tokio::signal;
use tracing::{error, info};

use crate::cache::{config_hash as cache_config_hash, BoltCache, Cache};
use crate::cleaner::{hard_delete, recycle_bin, DeleteResult};
use crate::config::Config;
use crate::rules::{Category, Engine, Finding};
use crate::scanner::{Progress, Scanner};
use crate::scanner_types::{FileRecord, Options, ScanError};

#[derive(RustEmbed)]
#[folder = "web/"]
struct WebAssets;

/// Shared server state
#[derive(Clone)]
pub struct ServerState {
    config: Arc<Mutex<Config>>,
    scanner: Arc<Mutex<Option<Scanner>>>,
    progress: Arc<Mutex<Option<Progress>>>,
    findings: Arc<Mutex<Vec<Finding>>>,
    records: Arc<Mutex<Vec<FileRecord>>>,
    errors: Arc<Mutex<Vec<ScanError>>>,
    scan_id: Arc<Mutex<u64>>,
    scan_done: Arc<Mutex<bool>>,
    cancel_flag: Arc<Mutex<bool>>,
}

impl ServerState {
    pub fn new(config: Config) -> Self {
        Self {
            config: Arc::new(Mutex::new(config)),
            scanner: Arc::new(Mutex::new(None)),
            progress: Arc::new(Mutex::new(None)),
            findings: Arc::new(Mutex::new(Vec::new())),
            records: Arc::new(Mutex::new(Vec::new())),
            errors: Arc::new(Mutex::new(Vec::new())),
            scan_id: Arc::new(Mutex::new(0)),
            scan_done: Arc::new(Mutex::new(false)),
            cancel_flag: Arc::new(Mutex::new(false)),
        }
    }
}

/// API request/response types
mod api {
    use super::*;
    use serde::{Deserialize, Serialize};

    #[derive(Deserialize)]
    pub struct ScanRequest {
        pub root: String,
        pub workers: Option<usize>,
        pub follow_links: bool,
        pub use_cache: bool,
        pub check_duplicates: bool,
        pub protect_system: bool,
    }

    #[derive(Serialize)]
    pub struct ScanResponse {
        pub scan_id: u64,
        pub status: String,
    }

    #[derive(Serialize)]
    pub struct ProgressResponse {
        pub progress: crate::ProgressSnapshot,
        pub done: bool,
    }

    #[derive(Deserialize)]
    pub struct ResultsParams {
        pub category: Option<String>,
        pub search: Option<String>,
        pub limit: Option<usize>,
        pub offset: Option<usize>,
    }

    #[derive(Serialize)]
    pub struct ResultsResponse {
        pub total: usize,
        pub filtered: usize,
        pub items: Vec<Finding>,
    }

    #[derive(Deserialize)]
    pub struct DeleteRequest {
        pub paths: Vec<String>,
        pub mode: String,
    }

    #[derive(Serialize)]
    pub struct DeleteResponse {
        pub deleted: usize,
        pub failed: usize,
        pub total_bytes: u64,
        pub errors: Vec<crate::cleaner::DeleteError>,
        pub success: bool,
    }

    #[derive(Serialize)]
    pub struct ConfigResponse {
        pub root: String,
        pub os: String,
        pub default_paths: Vec<String>,
        pub workers: usize,
        pub follow_links: bool,
        pub use_cache: bool,
        pub check_duplicates: bool,
        pub protect_system: bool,
        pub large_bytes: u64,
        pub huge_bytes: u64,
        pub stale_days: i64,
        pub old_log_days: i64,
        pub stale_install_days: i64,
        pub junk_extensions: Vec<String>,
        pub junk_dirs: Vec<String>,
        pub exclude_dirs: Vec<String>,
        pub exclude_prefix: Vec<String>,
        pub allow_protected: bool,
    }

    impl From<&Config> for ConfigResponse {
        fn from(c: &Config) -> Self {
            let os = std::env::consts::OS.to_string();
            let mut default_paths = Vec::new();

            #[cfg(target_os = "macos")]
            {
                if let Some(home) = dirs::home_dir() {
                    let home_str = home.to_string_lossy().to_string();
                    default_paths.push(home_str.clone());
                    default_paths.push(format!("{}/Downloads", home_str));
                    default_paths.push(format!("{}/Documents", home_str));
                    default_paths.push(format!("{}/Library/Caches", home_str));
                }
                default_paths.push("/".to_string());
            }

            #[cfg(target_os = "windows")]
            {
                for letter in b'C'..=b'Z' {
                    let drive = format!("{}:\\", letter as char);
                    if std::path::Path::new(&drive).exists() {
                        default_paths.push(drive);
                    }
                }
            }

            #[cfg(all(unix, not(target_os = "macos")))]
            {
                if let Some(home) = dirs::home_dir() {
                    default_paths.push(home.to_string_lossy().to_string());
                }
                default_paths.push("/".to_string());
            }

            Self {
                root: c.root.clone(),
                os,
                default_paths,
                workers: c.workers,
                follow_links: c.follow_links,
                use_cache: c.use_cache,
                check_duplicates: c.check_duplicates,
                protect_system: c.protect_system,
                large_bytes: c.large_bytes,
                huge_bytes: c.huge_bytes,
                stale_days: c.stale_days,
                old_log_days: c.old_log_days,
                stale_install_days: c.stale_install_days,
                junk_extensions: c.junk_extensions.clone(),
                junk_dirs: c.junk_dirs.clone(),
                exclude_dirs: c.exclude_dirs.clone(),
                exclude_prefix: c.exclude_prefix.clone(),
                allow_protected: c.allow_protected,
            }
        }
    }
}

/// Smart Scan API types
mod smart_api {
    use super::*;
    use serde::{Deserialize, Serialize};

    #[derive(Serialize)]
    pub struct SmartCategorySummary {
        pub category: Category,
        pub count: usize,
        pub total_size: u64,
        pub risk: crate::rules::Risk,
        pub description: String,
        pub paths_sample: Vec<String>,
    }

    #[derive(Serialize)]
    pub struct SmartScanResponse {
        pub scan_id: u64,
        pub status: String,
        pub categories: Vec<SmartCategorySummary>,
        pub total_reclaimable: u64,
        pub total_files: usize,
    }

    #[derive(Deserialize)]
    pub struct SmartCleanRequest {
        pub categories: Vec<String>, // Category names to clean
        pub mode: String,            // "recycle" or "hard"
    }

    #[derive(Serialize)]
    pub struct SmartCleanResponse {
        pub deleted: usize,
        pub failed: usize,
        pub total_bytes: u64,
        pub errors: Vec<crate::cleaner::DeleteError>,
        pub success: bool,
    }
}

use api::*;
use smart_api::*;

/// Create the router with all routes
pub fn create_router(state: ServerState) -> Router {
    Router::new()
        .route("/api/scan", post(handle_scan))
        .route("/api/stop", post(handle_stop))
        .route("/api/progress", get(handle_progress))
        .route("/api/results", get(handle_results))
        .route("/api/delete", post(handle_delete))
        .route("/api/config", get(handle_get_config).put(handle_put_config))
        .route("/api/export", get(handle_export))
        // Smart Scan endpoints
        .route("/api/smart-scan", post(handle_smart_scan))
        .route("/api/smart-categories", get(handle_smart_categories))
        .route("/api/smart-clean", post(handle_smart_clean))
        .fallback(handle_static)
        .layer(tower_http::cors::CorsLayer::permissive())
        .with_state(state)
}

/// Start the HTTP server and open the user's default browser
pub async fn run_server(config: Config) -> anyhow::Result<()> {
    let url = format!("http://127.0.0.1:{}", config.web_port);
    tokio::spawn(async move {
        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
        let _ = open::that(&url);
    });

    run_server_headless(config).await
}

/// Start the HTTP server without opening a browser (used by the desktop shell)
pub async fn run_server_headless(config: Config) -> anyhow::Result<()> {
    run_server_ready(config, |_| {}).await
}

/// Bind the server (resolving an ephemeral port when `web_port == 0`),
/// report the actually bound port via `on_ready`, then serve until shutdown.
pub async fn run_server_ready<F>(config: Config, on_ready: F) -> anyhow::Result<()>
where
    F: FnOnce(u16),
{
    let state = ServerState::new(config);
    let requested = state.config.lock().unwrap().web_port;
    let addr = SocketAddr::from(([127, 0, 0, 1], requested));

    let listener = tokio::net::TcpListener::bind(addr).await?;
    let actual = listener.local_addr()?.port();

    {
        let mut c = state.config.lock().unwrap();
        c.web_port = actual;
    }

    let app = create_router(state.clone());

    info!("Starting web server on http://127.0.0.1:{}", actual);

    on_ready(actual);

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    Ok(())
}

async fn shutdown_signal() {
    let ctrl_c = async {
        signal::ctrl_c()
            .await
            .expect("Failed to install Ctrl+C handler");
    };
    #[cfg(unix)]
    let terminate = async {
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("Failed to install signal handler")
            .recv()
            .await;
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }
    info!("Shutdown signal received");
}

async fn handle_scan(
    State(state): State<ServerState>,
    Json(req): Json<ScanRequest>,
) -> impl IntoResponse {
    let mut cfg = state.config.lock().unwrap().clone();

    cfg.root = req.root;
    if let Some(w) = req.workers {
        cfg.workers = w;
    }
    cfg.follow_links = req.follow_links;
    cfg.use_cache = req.use_cache;
    cfg.check_duplicates = req.check_duplicates;
    cfg.protect_system = req.protect_system;

    // Reset state
    let scan_id = {
        let mut scan_id = state.scan_id.lock().unwrap();
        *scan_id += 1;
        *scan_id
    };

    // Drop the previous scanner (and its open redb DB handle) BEFORE creating
    // a new one — redb locks the database file, so a second Database::create
    // on the same path fails silently unless the old handle is released first.
    // Also STOP the old walker so a previous long scan doesn't keep consuming
    // CPU/memory in the background.
    {
        let mut lock = state.scanner.lock().unwrap();
        if let Some(old) = lock.as_ref() {
            old.stop();
        }
        *lock = None;
    }

    *state.findings.lock().unwrap() = Vec::new();
    *state.records.lock().unwrap() = Vec::new();
    *state.errors.lock().unwrap() = Vec::new();
    *state.scan_done.lock().unwrap() = false;
    *state.cancel_flag.lock().unwrap() = false;

    let progress = Progress::new();
    *state.progress.lock().unwrap() = Some(progress.clone());

    let opts = Options {
        workers: cfg.workers,
        follow_links: cfg.follow_links,
        exclude: cfg.exclude_dirs.clone(),
        exclude_pref: cfg.exclude_prefix.clone(),
    };

    let cache: Option<Arc<dyn Cache>> = if cfg.use_cache {
        let hash = cache_config_hash(&opts);
        BoltCache::new("unused-removal", &hash)
            .ok()
            .map(|c| Arc::new(c) as Arc<dyn Cache>)
    } else {
        None
    };

    let scanner = Scanner::new(opts, progress, cache);
    *state.scanner.lock().unwrap() = Some(scanner.clone());

    let root = cfg.root.clone();
    let state_clone = state.clone();
    let cfg_clone = cfg.clone();

    tokio::spawn(async move {
        let root_for_walk = root.clone();
        let recs_result = tokio::task::spawn_blocking(move || scanner.walk(&root_for_walk)).await;

        let (recs, errs) = match recs_result {
            Ok(Ok((r, e))) => (r, e),
            Ok(Err(e)) => {
                error!("Scan error: {}", e);
                (
                    Vec::new(),
                    vec![ScanError {
                        path: root.clone(),
                        error: e.to_string(),
                    }],
                )
            }
            Err(e) => {
                error!("Scan task panicked: {}", e);
                (
                    Vec::new(),
                    vec![ScanError {
                        path: root,
                        error: "Scan panicked".to_string(),
                    }],
                )
            }
        };

        let engine = Engine::new(Arc::new(cfg_clone.clone()));
        let mut findings = engine.analyze(&recs);

        if cfg_clone.check_duplicates {
            let dups = engine.find_duplicates(&recs);
            findings.extend(dups);
        }

        findings.sort_by(|a, b| b.size.cmp(&a.size));

        if let Some(p) = state_clone.progress.lock().unwrap().as_ref() {
            p.finish();
        }
        *state_clone.records.lock().unwrap() = recs;
        *state_clone.errors.lock().unwrap() = errs;
        *state_clone.findings.lock().unwrap() = findings;
        *state_clone.scan_done.lock().unwrap() = true;
    });

    Json(ScanResponse {
        scan_id,
        status: "started".to_string(),
    })
}

async fn handle_stop(State(state): State<ServerState>) -> impl IntoResponse {
    let mut stopped = false;
    if let Some(scanner) = state.scanner.lock().unwrap().as_ref() {
        if !*state.scan_done.lock().unwrap() {
            *state.cancel_flag.lock().unwrap() = true;
            scanner.stop();
            stopped = true;
        }
    }
    Json(serde_json::json!({ "status": "stopped", "stopped": stopped }))
}

async fn handle_progress(State(state): State<ServerState>) -> Response {
    let progress = state.progress.lock().unwrap().clone();
    let done = *state.scan_done.lock().unwrap();

    if let Some(p) = progress {
        Json(ProgressResponse {
            progress: p.snapshot(),
            done,
        })
        .into_response()
    } else {
        (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": "no scan in progress" })),
        )
            .into_response()
    }
}

async fn handle_results(
    State(state): State<ServerState>,
    Query(params): Query<ResultsParams>,
) -> impl IntoResponse {
    let findings = state.findings.lock().unwrap().clone();

    let mut filtered = findings.clone();

    if let Some(cat) = params.category {
        if let Ok(category) = Category::from_str(&cat) {
            filtered.retain(|f| f.category == category);
        }
    }

    if let Some(search) = params.search {
        let search = search.to_lowercase();
        filtered.retain(|f| {
            f.path.to_lowercase().contains(&search) || f.reason.to_lowercase().contains(&search)
        });
    }

    let total = findings.len();
    let offset = params.offset.unwrap_or(0);
    let limit = params.limit.unwrap_or(filtered.len());

    let paginated: Vec<Finding> = filtered
        .clone()
        .into_iter()
        .skip(offset)
        .take(limit)
        .collect();

    Json(ResultsResponse {
        total,
        filtered: filtered.len(),
        items: paginated,
    })
}

async fn handle_delete(
    State(state): State<ServerState>,
    Json(req): Json<DeleteRequest>,
) -> Response {
    if req.paths.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": "no paths provided" })),
        )
            .into_response();
    }

    let result: Result<DeleteResult, anyhow::Error> = if req.mode == "hard" {
        hard_delete(&req.paths)
    } else {
        recycle_bin(&req.paths)
    };

    match result {
        Ok(res) => {
            let deleted_set: std::collections::HashSet<_> = res.deleted.iter().cloned().collect();
            state
                .findings
                .lock()
                .unwrap()
                .retain(|f| !deleted_set.contains(&f.path));

            let failed_count = res.failed.len();
            Json(DeleteResponse {
                deleted: res.deleted.len(),
                failed: failed_count,
                total_bytes: res.total_bytes,
                errors: res.failed,
                success: failed_count == 0,
            })
            .into_response()
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": e.to_string() })),
        )
            .into_response(),
    }
}

async fn handle_get_config(State(state): State<ServerState>) -> impl IntoResponse {
    let cfg = state.config.lock().unwrap().clone();
    Json(ConfigResponse::from(&cfg))
}

async fn handle_put_config(
    State(state): State<ServerState>,
    Json(update): Json<serde_json::Value>,
) -> Response {
    let mut cfg = state.config.lock().unwrap().clone();

    if let Some(v) = update.get("root").and_then(|x| x.as_str()) {
        cfg.root = v.to_string();
    }
    if let Some(v) = update.get("workers").and_then(|x| x.as_u64()) {
        cfg.workers = v as usize;
    }
    if let Some(v) = update.get("follow_links").and_then(|x| x.as_bool()) {
        cfg.follow_links = v;
    }
    if let Some(v) = update.get("use_cache").and_then(|x| x.as_bool()) {
        cfg.use_cache = v;
    }
    if let Some(v) = update.get("check_duplicates").and_then(|x| x.as_bool()) {
        cfg.check_duplicates = v;
    }
    if let Some(v) = update.get("protect_system").and_then(|x| x.as_bool()) {
        cfg.protect_system = v;
    }
    if let Some(v) = update.get("allow_protected").and_then(|x| x.as_bool()) {
        cfg.allow_protected = v;
    }
    if let Some(v) = update.get("web_port").and_then(|x| x.as_u64()) {
        cfg.web_port = v as u16;
    }

    *state.config.lock().unwrap() = cfg.clone();

    if let Err(e) = cfg.save(Path::new("config.toml")) {
        error!("Failed to save config: {}", e);
        return Json(serde_json::json!({ "status": "saved_memory", "warning": e.to_string() }))
            .into_response();
    }

    Json(serde_json::json!({ "status": "saved" })).into_response()
}

async fn handle_export(
    State(state): State<ServerState>,
    Query(params): Query<HashMap<String, String>>,
) -> Response {
    let format = params
        .get("format")
        .cloned()
        .unwrap_or_else(|| "json".to_string());
    let findings = state.findings.lock().unwrap().clone();

    if findings.is_empty() {
        return (StatusCode::BAD_REQUEST, "no scan results").into_response();
    }

    match format.as_str() {
        "csv" => {
            let mut csv = String::new();
            csv.push_str("path,size_bytes,category,reason,risk,mod_time\n");
            for f in &findings {
                csv.push_str(&format!(
                    "{},{},{},{},{},{}\n",
                    escape_csv(&f.path),
                    f.size,
                    f.category,
                    escape_csv(&f.reason),
                    format!("{:?}", f.risk),
                    format_time(f.mod_time),
                ));
            }
            Response::builder()
                .header("Content-Type", "text/csv")
                .header(
                    "Content-Disposition",
                    "attachment; filename=\"unused-removal-report.csv\"",
                )
                .body(Body::from(csv))
                .unwrap()
        }
        _ => {
            let json = serde_json::to_string_pretty(&findings).unwrap();
            Response::builder()
                .header("Content-Type", "application/json")
                .header(
                    "Content-Disposition",
                    "attachment; filename=\"unused-removal-report.json\"",
                )
                .body(Body::from(json))
                .unwrap()
        }
    }
}

async fn handle_static(State(_state): State<ServerState>, uri: axum::http::Uri) -> Response {
    let path = uri.path().trim_start_matches('/');
    let path = if path.is_empty() { "index.html" } else { path };

    match WebAssets::get(path) {
        Some(content) => {
            let mime = from_path(path).first_or_octet_stream();
            Response::builder()
                .header("Content-Type", mime.as_ref())
                .body(Body::from(content.data.to_vec()))
                .unwrap()
        }
        None => {
            if let Some(content) = WebAssets::get("index.html") {
                let mime = from_path("index.html").first_or_octet_stream();
                Response::builder()
                    .header("Content-Type", mime.as_ref())
                    .body(Body::from(content.data.to_vec()))
                    .unwrap()
            } else {
                (StatusCode::NOT_FOUND, "Not found").into_response()
            }
        }
    }
}

fn escape_csv(s: &str) -> String {
    if s.contains(',') || s.contains('"') || s.contains('\n') {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

fn format_time(t: std::time::SystemTime) -> String {
    let datetime: chrono::DateTime<chrono::Utc> = t.into();
    datetime.to_rfc3339()
}

// ========== Smart Scan Handlers ==========

async fn handle_smart_scan(State(state): State<ServerState>) -> impl IntoResponse {
    let mut cfg = state.config.lock().unwrap().clone();

    // Enable all smart junk categories for smart scan
    cfg.smart_junk_enabled = true;
    cfg.scan_user_caches = true;
    cfg.scan_system_logs = true;
    cfg.scan_language_files = true;
    cfg.scan_old_backups = true;
    cfg.scan_mail_attachments = true;
    cfg.scan_trash = true;
    cfg.scan_old_downloads = true;
    cfg.scan_unused_disk_images = true;
    cfg.scan_dev_caches = true;
    cfg.scan_ide_caches = true;
    cfg.scan_large_hidden = true;
    // NOTE: duplicate hashing is expensive and only surfaces in the
    // `aggressive` safety level — respect the caller's config instead of
    // forcing it on (a balanced scan of a large home dir must stay fast).

    // Increment scan id
    let scan_id = {
        let mut scan_id = state.scan_id.lock().unwrap();
        *scan_id += 1;
        *scan_id
    };

    // Reset state: stop any previous walker first so it can't keep running
    // (and consuming CPU/RAM) after being replaced by a new scan.
    {
        let mut lock = state.scanner.lock().unwrap();
        if let Some(old) = lock.as_ref() {
            old.stop();
        }
        *lock = None;
    }
    *state.findings.lock().unwrap() = Vec::new();
    *state.records.lock().unwrap() = Vec::new();
    *state.errors.lock().unwrap() = Vec::new();
    *state.scan_done.lock().unwrap() = false;
    *state.cancel_flag.lock().unwrap() = false;

    let progress = Progress::new();
    *state.progress.lock().unwrap() = Some(progress.clone());

    let opts = Options {
        workers: cfg.workers,
        follow_links: cfg.follow_links,
        exclude: cfg.exclude_dirs.clone(),
        exclude_pref: cfg.exclude_prefix.clone(),
    };

    let cache: Option<Arc<dyn Cache>> = if cfg.use_cache {
        let hash = cache_config_hash(&opts);
        BoltCache::new("unused-removal", &hash)
            .ok()
            .map(|c| Arc::new(c) as Arc<dyn Cache>)
    } else {
        None
    };

    let scanner = Scanner::new(opts, progress, cache);
    *state.scanner.lock().unwrap() = Some(scanner.clone());

    let root = cfg.root.clone();
    let state_clone = state.clone();
    let cfg_clone = cfg.clone();

    tokio::spawn(async move {
        // Smart scan walks known junk zones (fast mode) for safe/balanced
        // levels, or the full tree for aggressive / narrow custom targets.
        let walk_roots = smart_scan_roots(&root, &cfg_clone.smart_junk_safety_level);
        info!(
            "Smart scan targets ({}): {:?}",
            walk_roots.len(),
            walk_roots
        );

        let recs_result = tokio::task::spawn_blocking(move || {
            let mut all_recs: Vec<FileRecord> = Vec::new();
            let mut all_errs: Vec<ScanError> = Vec::new();
            for r in &walk_roots {
                if scanner.is_stopped() {
                    break;
                }
                match scanner.walk(r) {
                    Ok((mut recs, errs)) => {
                        all_recs.append(&mut recs);
                        all_errs.extend(errs);
                    }
                    Err(e) => {
                        error!("Scan error on {}: {}", r, e);
                        all_errs.push(ScanError {
                            path: r.clone(),
                            error: e.to_string(),
                        });
                    }
                }
            }
            (all_recs, all_errs)
        })
        .await;

        let (recs, errs) = match recs_result {
            Ok((r, e)) => (r, e),
            Err(e) => {
                error!("Scan task panicked: {}", e);
                (
                    Vec::new(),
                    vec![ScanError {
                        path: root.clone(),
                        error: "Scan panicked".to_string(),
                    }],
                )
            }
        };

        let engine = Engine::new(Arc::new(cfg_clone.clone()));
        let mut findings = engine.analyze(&recs);

        if cfg_clone.check_duplicates {
            let dups = engine.find_duplicates(&recs);
            findings.extend(dups);
        }

        findings.sort_by(|a, b| b.size.cmp(&a.size));

        // Filter by safety level
        findings = filter_findings_by_safety(findings, &cfg_clone);

        if let Some(p) = state_clone.progress.lock().unwrap().as_ref() {
            p.finish();
        }
        *state_clone.records.lock().unwrap() = recs;
        *state_clone.errors.lock().unwrap() = errs;
        *state_clone.findings.lock().unwrap() = findings;
        *state_clone.scan_done.lock().unwrap() = true;
    });

    Json(serde_json::json!({ "scan_id": scan_id, "status": "started" }))
}

async fn handle_smart_categories(State(state): State<ServerState>) -> impl IntoResponse {
    let findings = state.findings.lock().unwrap().clone();

    if findings.is_empty() {
        return Json(SmartScanResponse {
            scan_id: 0,
            status: "no_scan".to_string(),
            categories: Vec::new(),
            total_reclaimable: 0,
            total_files: 0,
        })
        .into_response();
    }

    let categories = build_category_summaries(&findings);
    let total_reclaimable: u64 = categories.iter().map(|c| c.total_size).sum();
    let total_files: usize = categories.iter().map(|c| c.count).sum();

    Json(SmartScanResponse {
        scan_id: *state.scan_id.lock().unwrap(),
        status: "completed".to_string(),
        categories,
        total_reclaimable,
        total_files,
    })
    .into_response()
}

async fn handle_smart_clean(
    State(state): State<ServerState>,
    Json(req): Json<SmartCleanRequest>,
) -> Response {
    if req.categories.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": "no categories provided" })),
        )
            .into_response();
    }

    let findings = state.findings.lock().unwrap().clone();

    // Filter findings by requested categories
    let category_set: std::collections::HashSet<String> = req.categories.into_iter().collect();
    let paths_to_delete: Vec<String> = findings
        .iter()
        .filter(|f| category_set.contains(&f.category.to_string()))
        .map(|f| f.path.clone())
        .collect();

    if paths_to_delete.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": "no matching files found" })),
        )
            .into_response();
    }

    let result: Result<DeleteResult, anyhow::Error> = if req.mode == "hard" {
        hard_delete(&paths_to_delete)
    } else {
        recycle_bin(&paths_to_delete)
    };

    match result {
        Ok(res) => {
            let deleted_set: std::collections::HashSet<_> = res.deleted.iter().cloned().collect();
            state
                .findings
                .lock()
                .unwrap()
                .retain(|f| !deleted_set.contains(&f.path));

            let failed_count = res.failed.len();
            Json(SmartCleanResponse {
                deleted: res.deleted.len(),
                failed: failed_count,
                total_bytes: res.total_bytes,
                errors: res.failed,
                success: failed_count == 0,
            })
            .into_response()
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": e.to_string() })),
        )
            .into_response(),
    }
}

/// Known junk zones for the fast smart-scan mode.
///
/// Safe/Balanced levels scan only these well-known sources (that is what
/// makes cleanup finish in seconds); Aggressive walks the full tree because
/// stale/huge/duplicate detection needs complete coverage. A narrow custom
/// target without standard zones falls back to a full walk of itself.
/// Enumerate existing filesystem roots (Windows: "C:\", "D:\", ...; Unix: "/").
fn all_root_volumes() -> Vec<String> {
    #[cfg(target_os = "windows")]
    {
        let mut drives = Vec::new();
        // Skip A:/B: which are usually floppy/empty slots that can hang on stat.
        for letter in 'C'..='Z' {
            let root = format!("{letter}:\\");
            if std::fs::exists(&root).unwrap_or(false) {
                drives.push(root);
            }
        }
        if drives.is_empty() {
            drives.push("C:\\".to_string());
        }
        drives
    }
    #[cfg(not(target_os = "windows"))]
    {
        vec!["/".to_string()]
    }
}

fn smart_scan_roots(root: &str, safety: &crate::config::SafetyLevel) -> Vec<String> {
    use crate::config::SafetyLevel as SL;

    if *safety == SL::Aggressive {
        // Aggressive walks the complete tree. When the user selected a drive root (e.g. "C:\"),
        // expand to every fixed drive so junk/large files on ALL disks are found; a narrow
        // custom folder still scans just that folder.
        if std::path::Path::new(root).parent().is_none() {
            return all_root_volumes();
        }
        return vec![root.to_string()];
    }

    let root_norm = root.trim_end_matches(['/', '\\']).to_string();
    let is_filesystem_root = std::path::Path::new(root).parent().is_none();

    // User-level junk (Downloads, caches, browser profiles) lives under the user's home/Profile
    // directory, NOT under an arbitrary scan root. When the user scans a drive root like "C:\",
    // "{root}\AppData\Local\Temp" resolves to "C:\AppData\..." which never exists on Windows — the
    // real location is "%USERPROFILE%\AppData\Local\Temp". So root user zones at the home dir when
    // root is a filesystem root, and stay relative to root otherwise (so a custom target folder
    // never "escapes" the folder the user actually selected).
    let home = dirs::home_dir()
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_else(|| root_norm.clone());
    let user_base = if is_filesystem_root { home } else { root_norm.clone() };

    let mut zones: Vec<String> = Vec::new();
    let mut push = |p: String| {
        if !zones.contains(&p) {
            zones.push(p);
        }
    };

    #[cfg(target_os = "macos")]
    {
        for rel in ["Downloads", "Library/Caches", "Library/Logs", ".Trash"] {
            push(format!("{user_base}/{rel}"));
        }
        if is_filesystem_root {
            push("/private/var/folders".to_string());
            push("/tmp".to_string());
        }
    }
    #[cfg(target_os = "windows")]
    {
        for rel in [
            r"Downloads",
            r"AppData\Local\Temp",
            r"AppData\Local\Google\Chrome\User Data\Default\Cache",
            r"AppData\Local\Microsoft\Edge\User Data\Default\Cache",
            r"AppData\Local\Mozilla\Firefox\Profiles",
        ] {
            push(format!("{user_base}\\{rel}"));
        }
        // Recycle Bin exists on each fixed drive — cover every one so trash on both disks is seen.
        if is_filesystem_root {
            for drive in all_root_volumes() {
                push(format!("{}\\$Recycle.Bin", drive.trim_end_matches(['/', '\\'])));
            }
        }
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        for rel in ["Downloads", ".cache", ".local/share/Trash"] {
            push(format!("{user_base}/{rel}"));
        }
        if is_filesystem_root {
            push("/tmp".to_string());
            push("/var/tmp".to_string());
        }
    }

    // Drop zones that don't exist or equal the root itself
    zones.retain(|z| {
        let zn = z.trim_end_matches(['/', '\\']);
        zn != root_norm && std::path::Path::new(z).exists()
    });

    if zones.is_empty() {
        vec![root.to_string()]
    } else {
        zones
    }
}

/// Filter findings based on safety level
fn filter_findings_by_safety(findings: Vec<Finding>, config: &Config) -> Vec<Finding> {
    use crate::config::SafetyLevel;
    use crate::rules::Category;

    let safety = config.smart_junk_safety_level;

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
            // Large files are informational (not auto-deleted), so surface them in the default
            // balanced scan too — users want to see what is taking up space on their disks.
            Category::Large,
            Category::Huge,
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
            Category::Huge,
            Category::Large,
        ],
    };

    findings
        .into_iter()
        .filter(|f| allowed_categories.contains(&f.category))
        .collect()
}

/// Build category summaries for smart scan response
fn build_category_summaries(findings: &[Finding]) -> Vec<SmartCategorySummary> {
    use crate::rules::Category;
    use std::collections::HashMap;

    let mut by_cat: HashMap<Category, Vec<&Finding>> = HashMap::new();
    for f in findings {
        by_cat.entry(f.category).or_default().push(f);
    }

    let mut summaries = Vec::new();

    for (category, items) in by_cat {
        let count = items.len();
        let total_size: u64 = items.iter().map(|f| f.size as u64).sum();
        let risk = items
            .first()
            .map(|f| f.risk)
            .unwrap_or(crate::rules::Risk::Safe);
        let description = get_category_description(category);
        let paths_sample = items.iter().take(5).map(|f| f.path.clone()).collect();

        summaries.push(SmartCategorySummary {
            category,
            count,
            total_size,
            risk,
            description,
            paths_sample,
        });
    }

    // Sort by total size descending
    summaries.sort_by(|a, b| b.total_size.cmp(&a.total_size));
    summaries
}

fn get_category_description(category: crate::rules::Category) -> String {
    use crate::rules::Category;
    match category {
        Category::UserCache => "Browser and application caches".to_string(),
        Category::SystemLog => "Old system and application logs".to_string(),
        Category::DevCache => "Development tool caches (npm, cargo, pip, gradle, etc.)".to_string(),
        Category::XcodeCache => "Xcode derived data, archives, device support".to_string(),
        Category::VSCodeCache => "VS Code / Cursor cached data and logs".to_string(),
        Category::Trash => "Files in Recycle Bin / Trash".to_string(),
        Category::OldDownload => "Old files in Downloads folder".to_string(),
        Category::Junk => "Temporary and junk files by extension/location".to_string(),
        Category::OldLog => "Old log files".to_string(),
        Category::StaleInstall => "Old installers in Downloads".to_string(),
        Category::LanguageFile => "Unused language/localization files".to_string(),
        Category::OldBackup => "Old iOS/Time Machine/Windows backups".to_string(),
        Category::MailAttachment => "Old mail attachments".to_string(),
        Category::UnusedDiskImage => "Unused disk images (.dmg, .iso, etc.)".to_string(),
        Category::LargeHidden => "Large hidden files".to_string(),
        Category::Stale => "Files not modified for a long time".to_string(),
        Category::Duplicate => "Duplicate files (same content)".to_string(),
        Category::AppLeftovers => "Possible leftover files from uninstalled apps".to_string(),
        Category::Huge => "Very large files".to_string(),
        Category::Large => "Large files".to_string(),
    }
}

#[cfg(test)]
mod smart_scan_root_tests {
    use super::smart_scan_roots;
    use crate::config::SafetyLevel;

    #[test]
    fn balanced_custom_root_never_escapes_selected_folder() {
        let root = tempfile::tempdir().unwrap();
        std::fs::create_dir(root.path().join("Downloads")).unwrap();
        let root_text = root.path().to_string_lossy().into_owned();

        let targets = smart_scan_roots(&root_text, &SafetyLevel::Balanced);

        assert!(!targets.is_empty());
        assert!(
            targets.iter().all(|target| target.starts_with(&root_text)),
            "{targets:?}"
        );
    }

    #[test]
    fn aggressive_scan_uses_the_complete_selected_tree() {
        let root = tempfile::tempdir().unwrap();
        let root_text = root.path().to_string_lossy().into_owned();

        assert_eq!(
            smart_scan_roots(&root_text, &SafetyLevel::Aggressive),
            vec![root_text]
        );
    }
}
