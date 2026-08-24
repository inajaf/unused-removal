//! Desktop shell: native window (tao + wry) around the embedded web UI.
//!
//! Enabled via the `desktop` cargo feature. The HTTP server runs unchanged on
//! 127.0.0.1 and the webview simply points at it — the entire existing code
//! path (scanner, rules, API, assets) is reused as-is.

use anyhow::Result;
use crate::config::Config;
use std::time::Duration;

/// Launch the desktop application: background server + native window.
///
/// Blocks until the user closes the window.
pub fn run_app(mut cfg: Config, port_override: Option<u16>) -> Result<()> {
    // Explicit --port wins; otherwise the OS assigns a free ephemeral port,
    // so an installed app can never collide with another instance or `serve`.
    cfg.web_port = port_override.unwrap_or(0);

    // 1) Server on a dedicated thread; reports its bound port back
    let (ready_tx, ready_rx) = std::sync::mpsc::channel::<u16>();
    let srv_cfg = cfg.clone();
    std::thread::Builder::new()
        .name("ur-server".into())
        .spawn(move || {
            let rt = tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()
                .expect("failed to build tokio runtime");
            let tx = ready_tx;
            if let Err(e) =
                rt.block_on(crate::server::run_server_ready(srv_cfg, move |port| {
                    let _ = tx.send(port);
                }))
            {
                eprintln!("[unused-removal] server error: {e:#}");
            }
        })?;

    // 2) Wait for the bound port, then open the native window
    let port = match ready_rx.recv_timeout(Duration::from_secs(15)) {
        Ok(p) => p,
        Err(_) => anyhow::bail!(
            "UI server did not start — try launching with --port <free port>"
        ),
    };
    show_window(&format!("http://127.0.0.1:{port}"))
}

#[cfg(feature = "desktop")]
fn show_window(url: &str) -> Result<()> {
    use tao::dpi::LogicalSize;
    use tao::event::{Event, WindowEvent};
    use tao::event_loop::{ControlFlow, EventLoop};
    use tao::window::WindowBuilder;

    let event_loop = EventLoop::new();
    let window = WindowBuilder::new()
        .with_title("unused-removal")
        .with_inner_size(LogicalSize::new(1180.0_f64, 780.0_f64))
        .with_min_inner_size(LogicalSize::new(760.0_f64, 560.0_f64))
        .build(&event_loop)?;

    let _webview = wry::WebViewBuilder::new()
        .with_url(url)
        .build(&window)?;

    // Blocks until the window is closed (`run` never returns).
    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::Wait;

        if let Event::WindowEvent { event: WindowEvent::CloseRequested, .. } = event {
            *control_flow = ControlFlow::Exit;
        }
    });
}

#[cfg(not(feature = "desktop"))]
fn show_window(_url: &str) -> Result<()> {
    anyhow::bail!(
        "This binary was built without the desktop shell.\n\
         Rebuild with: cargo build --release --features desktop"
    );
}
