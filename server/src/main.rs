use anyhow::{Context, Result};
use clap::Parser;
use rmcp::{transport::stdio, ServiceExt};
use tracing::info;
use tracing_subscriber::EnvFilter;

use mcp_pennylane::client::{probe_readonly_from_me, PennylaneClient};
use mcp_pennylane::config::{Cli, Config, Transport};
use mcp_pennylane::generated::SPEC_VERSION;
use mcp_pennylane::http_server;
use mcp_pennylane::server::PennylaneService;

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    init_tracing(cli.log_level.as_deref());

    let mut cfg = Config::from_cli(cli).context("invalid configuration")?;

    if cfg.needs_readonly_probe() {
        info!("PENNYLANE_READONLY not set, probing GET /me to auto-detect token scopes");
        let probe = PennylaneClient::new(&cfg.token, &cfg.base_url, cfg.api_2026)
            .map_err(|e| anyhow::anyhow!("{}", e))
            .context("failed to build probe HTTP client")?;
        let detected = probe_readonly_from_me(&probe)
            .await
            .map_err(|e| anyhow::anyhow!("{}", e))
            .context(
                "failed to auto-detect readonly mode via GET /me. \
                 Set PENNYLANE_READONLY=true|false explicitly to skip the probe.",
            )?;
        cfg.readonly = detected;
        info!(
            detected_readonly = detected,
            "readonly auto-detected from /me scopes"
        );
    }

    info!(
        version = env!("CARGO_PKG_VERSION"),
        spec_version = SPEC_VERSION,
        env = %cfg.environment,
        readonly = cfg.readonly,
        readonly_source = %cfg.readonly_source,
        api_2026 = cfg.api_2026,
        base_url = %cfg.base_url,
        "starting mcp-pennylane"
    );

    let service = PennylaneService::new(cfg.clone()).context("failed to build MCP service")?;

    info!(
        tool_count = service.exposed_tool_count(),
        "registered tools"
    );

    match cfg.transport {
        Transport::Stdio => {
            info!("transport=stdio");
            let server = service
                .serve(stdio())
                .await
                .context("failed to start stdio MCP server")?;
            server.waiting().await?;
        }
        Transport::Http => {
            info!(
                host = %cfg.http_host,
                port = cfg.http_port,
                "transport=streamable-http"
            );
            http_server::serve(service, &cfg.http_host, cfg.http_port)
                .await
                .context("failed to run streamable-http MCP server")?;
        }
    }

    info!("mcp-pennylane stopped");
    Ok(())
}

fn init_tracing(cli_log_level: Option<&str>) {
    let env_filter = if let Some(level) = cli_log_level {
        EnvFilter::try_new(level).unwrap_or_else(|_| EnvFilter::new("info"))
    } else {
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"))
    };

    tracing_subscriber::fmt()
        .with_env_filter(env_filter)
        .with_writer(std::io::stderr)
        .init();
}
