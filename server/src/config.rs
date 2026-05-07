use anyhow::{anyhow, Result};
use clap::Parser;
use std::fmt;

#[derive(Parser, Debug)]
#[command(
    name = "mcp-pennylane",
    version,
    about = "MCP server for the Pennylane Company API v2"
)]
pub struct Cli {
    /// Pennylane Company API token (Bearer). Generated in Pennylane → Settings → Connectivity → Developers.
    #[arg(long, env = "PENNYLANE_API_KEY")]
    pub token: Option<String>,

    /// Override the Pennylane base URL.
    #[arg(
        long,
        env = "PENNYLANE_BASE_URL",
        default_value = "https://app.pennylane.com"
    )]
    pub base_url: String,

    /// Filter every non-GET operation at registration time. When unset the
    /// server probes `GET /me` at startup and forces readonly if every token
    /// scope ends with `:readonly`.
    #[arg(long, env = "PENNYLANE_READONLY")]
    pub readonly: Option<bool>,

    /// Visual hint shown in the startup banner and the augmented `getMe` response.
    #[arg(long, env = "PENNYLANE_ENV", default_value = "production")]
    pub env: String,

    /// Send the `X-Use-2026-API-Changes: true` header on every request.
    #[arg(long, env = "PENNYLANE_API_2026", default_value_t = false)]
    pub api_2026: bool,

    /// Transport. `stdio` (default) or `http` (streamable HTTP).
    #[arg(short, long, env = "MCP_PENNYLANE_TRANSPORT", default_value = "stdio")]
    pub transport: String,

    /// HTTP host for the streamable HTTP transport.
    #[arg(long, default_value = "127.0.0.1")]
    pub host: String,

    /// HTTP port for the streamable HTTP transport.
    #[arg(long, default_value_t = 8000)]
    pub port: u16,

    /// Override the tracing env-filter (e.g. `info`, `mcp_pennylane=debug`).
    #[arg(long)]
    pub log_level: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Transport {
    Stdio,
    Http,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Environment {
    Production,
    Sandbox,
}

impl fmt::Display for Environment {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Environment::Production => f.write_str("production"),
            Environment::Sandbox => f.write_str("sandbox"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReadonlySource {
    /// User explicitly set `PENNYLANE_READONLY` (env or CLI).
    Explicit,
    /// Resolved at startup by probing `GET /me` and inspecting token scopes.
    AutoDetected,
}

impl fmt::Display for ReadonlySource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ReadonlySource::Explicit => f.write_str("explicit"),
            ReadonlySource::AutoDetected => f.write_str("auto"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Config {
    pub token: String,
    pub base_url: String,
    pub readonly: bool,
    pub readonly_source: ReadonlySource,
    pub environment: Environment,
    pub api_2026: bool,
    pub transport: Transport,
    pub http_host: String,
    pub http_port: u16,
}

impl Config {
    /// Build a tentative config from CLI/env. `readonly` defaults to `false`
    /// when not explicitly set; main.rs must call `resolve_readonly_auto()` to
    /// upgrade the value via the `/me` scope probe.
    pub fn from_cli(cli: Cli) -> Result<Self> {
        let token = cli
            .token
            .ok_or_else(|| anyhow!("missing PENNYLANE_API_KEY (env or --token)"))?;

        if token.trim().is_empty() {
            return Err(anyhow!("PENNYLANE_API_KEY is empty"));
        }

        let environment = match cli.env.to_lowercase().as_str() {
            "production" | "prod" => Environment::Production,
            "sandbox" | "test" => Environment::Sandbox,
            other => {
                return Err(anyhow!(
                    "PENNYLANE_ENV must be `production` or `sandbox`, got `{}`",
                    other
                ))
            }
        };

        let transport = match cli.transport.to_lowercase().as_str() {
            "stdio" => Transport::Stdio,
            "http" | "streamable-http" => Transport::Http,
            other => {
                return Err(anyhow!(
                    "transport must be `stdio` or `http`, got `{}`",
                    other
                ))
            }
        };

        let (readonly, readonly_source) = match cli.readonly {
            Some(v) => (v, ReadonlySource::Explicit),
            None => (false, ReadonlySource::AutoDetected),
        };

        Ok(Self {
            token,
            base_url: cli.base_url.trim_end_matches('/').to_string(),
            readonly,
            readonly_source,
            environment,
            api_2026: cli.api_2026,
            transport,
            http_host: cli.host,
            http_port: cli.port,
        })
    }

    /// True if the user did not pin `PENNYLANE_READONLY` and we should probe
    /// `/me` at startup to auto-detect from token scopes.
    pub fn needs_readonly_probe(&self) -> bool {
        self.readonly_source == ReadonlySource::AutoDetected
    }
}

/// Pure scope-classification helper. A token is considered read-only iff its
/// scope list is non-empty AND every scope ends with `:readonly`. This matches
/// the Pennylane v2 scope convention where each scope has a `:readonly`
/// counterpart (e.g., `customers:readonly` vs `customers:all`).
pub fn scopes_imply_readonly(scopes: &[String]) -> bool {
    if scopes.is_empty() {
        return false;
    }
    scopes.iter().all(|s| s.ends_with(":readonly"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(items: &[&str]) -> Vec<String> {
        items.iter().map(|x| x.to_string()).collect()
    }

    #[test]
    fn empty_scopes_are_not_readonly() {
        assert!(!scopes_imply_readonly(&[]));
    }

    #[test]
    fn all_readonly_scopes_classify_readonly() {
        assert!(scopes_imply_readonly(&s(&["customers:readonly"])));
        assert!(scopes_imply_readonly(&s(&[
            "customers:readonly",
            "ledger_accounts:readonly",
            "trial_balance:readonly",
        ])));
    }

    #[test]
    fn any_write_scope_disqualifies() {
        assert!(!scopes_imply_readonly(&s(&[
            "customers:readonly",
            "customers:all"
        ])));
        assert!(!scopes_imply_readonly(&s(&["customers:all"])));
    }

    #[test]
    fn legacy_unsuffixed_scope_disqualifies() {
        // The 2026 changes deprecate the bundled `ledger` scope. Until then it
        // still grants writes — treat as non-readonly.
        assert!(!scopes_imply_readonly(&s(&["ledger"])));
        assert!(!scopes_imply_readonly(&s(&[
            "customers:readonly",
            "ledger"
        ])));
    }

    #[test]
    fn export_action_scope_disqualifies() {
        // `exports:fec` triggers an export — it's a write action.
        assert!(!scopes_imply_readonly(&s(&["exports:fec"])));
    }
}
