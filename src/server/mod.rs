//! HTTP server with embedded web UI and REST API

use std::net::SocketAddr;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::collections::HashMap;
use std::str::FromStr;

use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::{IntoResponse, Json, Response},
    routing::{get, post, put},
    Router,
};
use axum::body::Body;
use rust_embed::RustEmbed;
use mime_guess::from_path;
use tokio::signal;
use tracing::{info, error};

use crate::config::Config;
use crate::scanner::{Walker, Options, Progress, FileRecord, ScanError};
use crate::cache::{Cache, BoltCache, config_hash as cache_config_hash};
use crate::rules::{Engine, Finding, Category, Risk};
use crate::cleaner::{recycle_bin, hard_delete, DeleteResult};

#[derive(RustEmbed)]
#[folder = "web/"]
struct WebAssets;

/// Shared server state
#[derive(Clone)]
pub struct ServerState {
    config: Arc<Mutex<Config>>,
    scanner: Arc<Mutex<Option<Walker>>>,
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
    use serde::{Deserialize, Serialize};
    use super::*;

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
        pub progress: crate::scanner::ProgressSnapshot,
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
            Self {
                root: c.root.clone(),
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

use api::*;

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
        .fallback(handle_static)
        .with_state(state)
}

/// Start the HTTP server
pub async fn run_server(config: Config) -> anyhow::Result<()> {
    let state = ServerState::new(config);
    let port = state.config.lock().unwrap().web_port;
    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    
    let app = create_router(state.clone());
    
    info!("Starting web server on http://{}", addr);
    
    let url = format!("http://{}", addr);
    tokio::spawn(async move {
        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
        let _ = open::that(&url);
    });

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).with_graceful_shutdown(shutdown_signal()).await?;
    
    Ok(())
}

async fn shutdown_signal() {
    let ctrl_c = async {
        signal::ctrl_c().await.expect("Failed to install Ctrl+C handler");
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
    if let Some(w) = req.workers { cfg.workers = w; }
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
        BoltCache::new("unused-removal", &hash).ok().map(Arc::new)
    } else { None };

    let walker = Walker::new(opts, progress, cache);
    *state.scanner.lock().unwrap() = Some(walker.clone());
    
    let root = cfg.root.clone();
    let state_clone = state.clone();
    let cfg_clone = cfg.clone();
    
    tokio::spawn(async move {
        let recs_result = tokio::task::spawn_blocking(move || walker.walk(&root)).await;
        
        let (recs, errs) = match recs_result {
            Ok(Ok((r, e))) => (r, e),
            Ok(Err(e)) => {
                error!("Scan error: {}", e);
                (Vec::new(), vec![ScanError { path: root, error: e.to_string() }])
            }
            Err(e) => {
                error!("Scan task panicked: {}", e);
                (Vec::new(), vec![ScanError { path: root, error: "Scan panicked".to_string() }])
            }
        };

        let engine = Engine::new(Arc::new(cfg_clone));
        let mut findings = engine.analyze(&recs);
        
        if cfg_clone.check_duplicates {
            let dups = engine.find_duplicates(&recs);
            findings.extend(dups);
        }
        
        findings.sort_by(|a, b| b.size.cmp(&a.size));
        
        *state_clone.records.lock().unwrap() = recs;
        *state_clone.errors.lock().unwrap() = errs;
        *state_clone.findings.lock().unwrap() = findings;
        *state_clone.scan_done.lock().unwrap() = true;
        if let Some(p) = state_clone.progress.lock().unwrap().as_ref() {
            p.finish();
        }
    });
    
    Json(ScanResponse { scan_id, status: "started".to_string() })
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

async fn handle_progress(State(state): State<ServerState>) -> impl IntoResponse {
    let progress = state.progress.lock().unwrap().clone();
    let done = *state.scan_done.lock().unwrap();
    
    if let Some(p) = progress {
        Json(ProgressResponse { progress: p.snapshot(), done })
    } else {
        (StatusCode::BAD_REQUEST, Json(serde_json::json!({ "error": "no scan in progress" })))
    }
}

async fn handle_results(
    State(state): State<ServerState>,
    Query(params): Query<ResultsParams>,
) -> impl IntoResponse {
    let findings = state.findings.lock().unwrap().clone();
    
    let mut filtered = findings;
    
    if let Some(cat) = params.category {
        if let Ok(category) = Category::from_str(&cat) {
            filtered.retain(|f| f.category == category);
        }
    }
    
    if let Some(search) = params.search {
        let search = search.to_lowercase();
        filtered.retain(|f| {
            f.path.to_lowercase().contains(&search) || 
            f.reason.to_lowercase().contains(&search)
        });
    }
    
    let total = findings.len();
    let offset = params.offset.unwrap_or(0);
    let limit = params.limit.unwrap_or(filtered.len());
    
    let paginated: Vec<Finding> = filtered.into_iter()
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
) -> impl IntoResponse {
    if req.paths.is_empty() {
        return (StatusCode::BAD_REQUEST, Json(serde_json::json!({ "error": "no paths provided" }))).into_response();
    }
    
    let result: Result<DeleteResult, anyhow::Error> = if req.mode == "hard" {
        hard_delete(&req.paths)
    } else {
        recycle_bin(&req.paths)
    };
    
    match result {
        Ok(res) => {
            let deleted_set: std::collections::HashSet<_> = res.deleted.iter().cloned().collect();
            state.findings.lock().unwrap().retain(|f| !deleted_set.contains(&f.path));
            
            let failed_count = res.failed.len();
            Json(DeleteResponse {
                deleted: res.deleted.len(),
                failed: failed_count,
                total_bytes: res.total_bytes,
                errors: res.failed,
                success: failed_count == 0,
            }).into_response()
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": e.to_string() }))
        ).into_response()
    }
}

async fn handle_get_config(State(state): State<ServerState>) -> impl IntoResponse {
    let cfg = state.config.lock().unwrap().clone();
    Json(ConfigResponse::from(&cfg))
}

async fn handle_put_config(
    State(state): State<ServerState>,
    Json(cfg): Json<Config>,
) -> impl IntoResponse {
    *state.config.lock().unwrap() = cfg.clone();
    
    if let Err(e) = cfg.save(Path::new("config.toml")) {
        error!("Failed to save config: {}", e);
        return (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({ "error": e.to_string() }))).into_response();
    }
    
    Json(serde_json::json!({ "status": "saved" }))
}

async fn handle_export(
    State(state): State<ServerState>,
    Query(params): Query<HashMap<String, String>>,
) -> impl IntoResponse {
    let format = params.get("format").cloned().unwrap_or_else(|| "json".to_string());
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
                .header("Content-Disposition", "attachment; filename=\"unused-removal-report.csv\"")
                .body(Body::from(csv))
                .unwrap()
        }
        _ => {
            let json = serde_json::to_string_pretty(&findings).unwrap();
            Response::builder()
                .header("Content-Type", "application/json")
                .header("Content-Disposition", "attachment; filename=\"unused-removal-report.json\"")
                .body(Body::from(json))
                .unwrap()
        }
    }
}

async fn handle_static(
    State(_state): State<ServerState>,
    axum::extract::Path(path): axum::extract::Path<String>,
) -> impl IntoResponse {
    let path = if path.is_empty() || path == "/" { "index.html" } else { &path };
    
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
    } else { s.to_string() }
}

fn format_time(t: std::time::SystemTime) -> String {
    let datetime: chrono::DateTime<chrono::Utc> = t.into();
    datetime.to_rfc3339()
}