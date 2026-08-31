//! Native desktop shell for the embedded UI.
//!
//! The desktop build uses a private WebView custom protocol. No browser is opened and no TCP
//! listener, network address, or port is created.

use crate::config::Config;
use anyhow::Result;

#[cfg(feature = "desktop")]
pub fn run_app(cfg: Config) -> Result<()> {
    use crate::server::{create_router, ServerState};
    use std::sync::Arc;
    use tao::dpi::LogicalSize;
    use tao::event::{Event, WindowEvent};
    use tao::event_loop::{ControlFlow, EventLoop};
    use tao::window::WindowBuilder;
    use wry::http::Request;

    let runtime = Arc::new(
        tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()?,
    );
    let router = create_router(ServerState::new(cfg));

    let event_loop = EventLoop::new();
    let window = WindowBuilder::new()
        .with_title("unused-removal")
        .with_inner_size(LogicalSize::new(1180.0_f64, 780.0_f64))
        .with_min_inner_size(LogicalSize::new(760.0_f64, 560.0_f64))
        .build(&event_loop)?;

    let protocol_runtime = runtime.clone();
    let protocol_router = router.clone();
    let _webview = wry::WebViewBuilder::new()
        .with_asynchronous_custom_protocol(
            "unused-removal".into(),
            move |_webview_id, request: Request<Vec<u8>>, responder| {
                let runtime = protocol_runtime.clone();
                let router = protocol_router.clone();
                runtime.spawn(async move {
                    responder.respond(route_desktop_request(router, request).await);
                });
            },
        )
        .with_url("unused-removal://app/")
        .build(&window)?;

    // Keep both the WebView and the Tokio runtime alive until the native window closes.
    event_loop.run(move |event, _, control_flow| {
        let _keep_runtime_alive = &runtime;
        *control_flow = ControlFlow::Wait;

        if let Event::WindowEvent {
            event: WindowEvent::CloseRequested,
            ..
        } = event
        {
            *control_flow = ControlFlow::Exit;
        }
    });
}

#[cfg(feature = "desktop")]
async fn route_desktop_request(
    router: axum::Router,
    request: wry::http::Request<Vec<u8>>,
) -> wry::http::Response<Vec<u8>> {
    use http_body_util::BodyExt;
    use tower::ServiceExt;

    let (parts, bytes) = request.into_parts();
    let request = axum::http::Request::from_parts(parts, axum::body::Body::from(bytes));
    let response = match router.oneshot(request).await {
        Ok(response) => response,
        Err(never) => match never {},
    };
    let (parts, body) = response.into_parts();
    let bytes = match body.collect().await {
        Ok(collected) => collected.to_bytes().to_vec(),
        Err(error) => {
            return wry::http::Response::builder()
                .status(500)
                .header("Content-Type", "text/plain; charset=utf-8")
                .body(format!("Desktop protocol error: {error}").into_bytes())
                .unwrap();
        }
    };

    let mut builder = wry::http::Response::builder().status(parts.status);
    for (name, value) in &parts.headers {
        builder = builder.header(name, value);
    }
    builder.body(bytes).unwrap()
}

#[cfg(not(feature = "desktop"))]
pub fn run_app(_cfg: Config) -> Result<()> {
    anyhow::bail!(
        "This binary was built without the desktop shell.\n\
         Rebuild with: cargo build --release --features desktop"
    );
}

#[cfg(all(test, feature = "desktop"))]
mod tests {
    use super::route_desktop_request;
    use crate::config::Config;
    use crate::server::{create_router, ServerState};

    #[tokio::test]
    async fn internal_protocol_serves_ui_and_api_without_a_socket() {
        let router = create_router(ServerState::new(Config::default()));

        let index = route_desktop_request(
            router.clone(),
            wry::http::Request::builder()
                .uri("unused-removal://app/")
                .body(Vec::new())
                .unwrap(),
        )
        .await;
        assert_eq!(index.status(), 200);
        assert!(index.body().starts_with(b"<!DOCTYPE html>"));
        assert!(index
            .body()
            .windows(b"data-language=\"en\"".len())
            .any(|window| window == b"data-language=\"en\""));

        let translations = route_desktop_request(
            router.clone(),
            wry::http::Request::builder()
                .uri("unused-removal://app/i18n.js")
                .body(Vec::new())
                .unwrap(),
        )
        .await;
        assert_eq!(translations.status(), 200);
        assert!(translations
            .body()
            .windows(b"initLanguageToggle".len())
            .any(|window| window == b"initLanguageToggle"));

        let config = route_desktop_request(
            router,
            wry::http::Request::builder()
                .uri("unused-removal://app/api/config")
                .body(Vec::new())
                .unwrap(),
        )
        .await;
        assert_eq!(config.status(), 200);
        let json: serde_json::Value = serde_json::from_slice(config.body()).unwrap();
        assert_eq!(json["os"], std::env::consts::OS);
    }
}
