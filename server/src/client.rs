use reqwest::header::{HeaderMap, HeaderName, HeaderValue, AUTHORIZATION, USER_AGENT};
use reqwest::{Client, Method};
use serde_json::Value;
use std::time::Duration;
use tracing::{debug, info, warn};

use crate::config::scopes_imply_readonly;
use crate::error::ClientError;
use crate::redact::redact_bearer;

const REQUEST_TIMEOUT_SECS: u64 = 60;
const MAX_RETRY_AFTER_SECS: u64 = 60;

#[derive(Clone)]
pub struct PennylaneClient {
    http: Client,
    base_url: String,
    token_redacted: String,
}

impl PennylaneClient {
    pub fn new(token: &str, base_url: &str, api_2026: bool) -> Result<Self, ClientError> {
        let mut headers = HeaderMap::new();

        let bearer = format!("Bearer {}", token);
        let mut auth_value = HeaderValue::from_str(&bearer)
            .map_err(|e| ClientError::Network(format!("invalid bearer token: {}", e)))?;
        auth_value.set_sensitive(true);
        headers.insert(AUTHORIZATION, auth_value);

        headers.insert(
            HeaderName::from_static("accept"),
            HeaderValue::from_static("application/json"),
        );

        if api_2026 {
            headers.insert(
                HeaderName::from_static("x-use-2026-api-changes"),
                HeaderValue::from_static("true"),
            );
        }

        let user_agent = format!("mcp-pennylane/{}", env!("CARGO_PKG_VERSION"));
        headers.insert(
            USER_AGENT,
            HeaderValue::from_str(&user_agent)
                .map_err(|e| ClientError::Network(format!("invalid user agent: {}", e)))?,
        );

        let http = Client::builder()
            .timeout(Duration::from_secs(REQUEST_TIMEOUT_SECS))
            .default_headers(headers)
            .build()
            .map_err(|e| ClientError::Network(format!("failed to build HTTP client: {}", e)))?;

        Ok(Self {
            http,
            base_url: base_url.trim_end_matches('/').to_string(),
            token_redacted: redact_bearer(token),
        })
    }

    pub fn token_redacted(&self) -> &str {
        &self.token_redacted
    }

    pub async fn call(
        &self,
        method: &str,
        path: &str,
        query: &[(String, String)],
        body: Option<Value>,
    ) -> Result<Value, ClientError> {
        let url = format!("{}{}", self.base_url, path);
        let http_method = method
            .parse::<Method>()
            .map_err(|e| ClientError::InvalidArgs {
                operation_id: path.to_string(),
                message: format!("unsupported HTTP method `{}`: {}", method, e),
            })?;

        let started = std::time::Instant::now();

        let mut response = self
            .send_once(&http_method, &url, query, body.as_ref())
            .await?;
        let mut status = response.status().as_u16();

        if status == 429 {
            let retry_after = parse_retry_after(response.headers()).min(MAX_RETRY_AFTER_SECS);
            warn!(
                method = method,
                path = path,
                retry_after_seconds = retry_after,
                "rate limited (429), retrying once"
            );
            tokio::time::sleep(Duration::from_secs(retry_after)).await;
            response = self
                .send_once(&http_method, &url, query, body.as_ref())
                .await?;
            status = response.status().as_u16();
        }

        log_rate_limit(response.headers());

        if (200..300).contains(&status) {
            let elapsed_ms = started.elapsed().as_millis();
            info!(
                method = method,
                path = path,
                status = status,
                duration_ms = elapsed_ms,
                "ok"
            );
            return parse_json_or_empty(response).await;
        }

        let retry_after = if status == 429 {
            Some(parse_retry_after(response.headers()).min(MAX_RETRY_AFTER_SECS))
        } else {
            None
        };
        let body_value = parse_json_or_empty(response).await.unwrap_or(Value::Null);
        let elapsed_ms = started.elapsed().as_millis();
        warn!(
            method = method,
            path = path,
            status = status,
            duration_ms = elapsed_ms,
            "error"
        );

        Err(ClientError::Http {
            status,
            body: body_value,
            retry_after_seconds: retry_after,
        })
    }

    async fn send_once(
        &self,
        method: &Method,
        url: &str,
        query: &[(String, String)],
        body: Option<&Value>,
    ) -> Result<reqwest::Response, ClientError> {
        let mut req = self.http.request(method.clone(), url);
        if !query.is_empty() {
            req = req.query(query);
        }
        if let Some(b) = body {
            req = req.json(b);
        }
        req.send()
            .await
            .map_err(|e| ClientError::Network(e.to_string()))
    }
}

async fn parse_json_or_empty(response: reqwest::Response) -> Result<Value, ClientError> {
    let bytes = response
        .bytes()
        .await
        .map_err(|e| ClientError::Network(e.to_string()))?;
    if bytes.is_empty() {
        return Ok(Value::Object(serde_json::Map::new()));
    }
    serde_json::from_slice::<Value>(&bytes).map_err(|e| {
        ClientError::Network(format!(
            "failed to parse Pennylane JSON ({} bytes): {}",
            bytes.len(),
            e
        ))
    })
}

fn parse_retry_after(headers: &HeaderMap) -> u64 {
    headers
        .get("retry-after")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(5)
}

/// Probe `GET /api/external/v2/me`, read the token's `scopes` field, and
/// classify the token as read-only iff every scope ends with `:readonly`.
///
/// Returns `false` (read+write) when the response has no `scopes` array — the
/// API does not always populate it, and we never fall back to `true` from
/// uncertainty (could surprise users expecting their write-scoped token to
/// work).
pub async fn probe_readonly_from_me(client: &PennylaneClient) -> Result<bool, ClientError> {
    let response = client.call("GET", "/api/external/v2/me", &[], None).await?;

    let scopes: Vec<String> = response
        .get("scopes")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|item| item.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default();

    Ok(scopes_imply_readonly(&scopes))
}

fn log_rate_limit(headers: &HeaderMap) {
    let limit = headers.get("ratelimit-limit").and_then(|v| v.to_str().ok());
    let remaining = headers
        .get("ratelimit-remaining")
        .and_then(|v| v.to_str().ok());
    if limit.is_some() || remaining.is_some() {
        debug!(
            limit = limit.unwrap_or("?"),
            remaining = remaining.unwrap_or("?"),
            "rate limit headers"
        );
    }
}
