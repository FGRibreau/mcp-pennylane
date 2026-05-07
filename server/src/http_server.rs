//! Streamable HTTP transport for the MCP service.
//!
//! Mounts `rmcp`'s `StreamableHttpService` at `/mcp` and adds a `GET /health`
//! probe. Single-tenant per process: every session shares the same
//! `PENNYLANE_API_KEY` configured at startup. Authentication between the MCP
//! client and this server is intentionally none — bind to `127.0.0.1` and put
//! a reverse proxy in front for any non-local exposure.

use std::sync::Arc;
use std::time::Duration;

use axum::routing::get;
use axum::Router;
use rmcp::transport::streamable_http_server::{
    session::local::LocalSessionManager, StreamableHttpServerConfig, StreamableHttpService,
};
use tokio_util::sync::CancellationToken;
use tracing::info;

use crate::server::PennylaneService;

const KEEPALIVE_INTERVAL: Duration = Duration::from_secs(15);

pub async fn serve(service: PennylaneService, host: &str, port: u16) -> anyhow::Result<()> {
    let cancel = CancellationToken::new();

    let factory_service = service.clone();
    let mcp_service = StreamableHttpService::new(
        move || Ok(factory_service.clone()),
        Arc::new(LocalSessionManager::default()),
        StreamableHttpServerConfig {
            sse_keep_alive: Some(KEEPALIVE_INTERVAL),
            stateful_mode: true,
            cancellation_token: cancel.clone(),
        },
    );

    let app = Router::new()
        .route("/health", get(health_handler))
        .nest_service("/mcp", mcp_service);

    let bind = format!("{}:{}", host, port);
    let listener = tokio::net::TcpListener::bind(&bind)
        .await
        .map_err(|e| anyhow::anyhow!("failed to bind {}: {}", bind, e))?;

    info!(
        host = host,
        port = port,
        "MCP endpoint http://{bind}/mcp · health http://{bind}/health"
    );

    let shutdown = async move {
        let _ = tokio::signal::ctrl_c().await;
        info!("ctrl-c received, shutting down");
        cancel.cancel();
    };

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown)
        .await
        .map_err(|e| anyhow::anyhow!("HTTP server error: {}", e))?;

    Ok(())
}

async fn health_handler() -> &'static str {
    "ok"
}
