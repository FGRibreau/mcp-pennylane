use serde_json::{json, Value};

const MAX_BODY_SNIPPET_BYTES: usize = 2_048;

#[derive(thiserror::Error, Debug)]
pub enum ClientError {
    #[error("network error: {0}")]
    Network(String),
    #[error("HTTP {status}")]
    Http {
        status: u16,
        body: Value,
        retry_after_seconds: Option<u64>,
    },
    #[error("the server is in read-only mode (PENNYLANE_READONLY=true); refused to dispatch {operation_id}")]
    Readonly {
        operation_id: String,
        method: String,
    },
    #[error("unknown tool `{0}`")]
    UnknownTool(String),
    #[error("invalid arguments for `{operation_id}`: {message}")]
    InvalidArgs {
        operation_id: String,
        message: String,
    },
}

impl ClientError {
    /// Render as the structured JSON object Claude expects in `CallToolResult`.
    pub fn to_mcp_value(&self) -> Value {
        match self {
            ClientError::Network(msg) => json!({
                "error": {
                    "code": "NETWORK_ERROR",
                    "message": msg,
                    "suggestion": "Check connectivity to https://app.pennylane.com and retry."
                }
            }),
            ClientError::Http {
                status,
                body,
                retry_after_seconds,
            } => {
                let (code, suggestion) = classify(*status);
                let mut err = serde_json::Map::new();
                err.insert("code".into(), json!(code));
                err.insert(
                    "message".into(),
                    json!(format!("Pennylane responded {}", status)),
                );
                err.insert("status".into(), json!(status));
                err.insert("suggestion".into(), json!(suggestion));
                err.insert("upstream_body".into(), truncate_body(body));
                if let Some(secs) = retry_after_seconds {
                    err.insert("retry_after_seconds".into(), json!(secs));
                }
                json!({ "error": err })
            }
            ClientError::Readonly {
                operation_id,
                method,
            } => json!({
                "error": {
                    "code": "READONLY_MODE",
                    "message": "Server is in read-only mode (PENNYLANE_READONLY=true).",
                    "operation_id": operation_id,
                    "operation_method": method,
                    "suggestion": "Restart with PENNYLANE_READONLY=false (and a write-scoped Pennylane token) to enable writes."
                }
            }),
            ClientError::UnknownTool(name) => json!({
                "error": {
                    "code": "UNKNOWN_TOOL",
                    "message": format!("No Pennylane operation named `{}`.", name),
                    "suggestion": "Use `pennylane_search_tools` to discover available operationIds."
                }
            }),
            ClientError::InvalidArgs {
                operation_id,
                message,
            } => json!({
                "error": {
                    "code": "INVALID_ARGS",
                    "message": message,
                    "operation_id": operation_id,
                    "suggestion": "Check the tool's input schema (visible via `pennylane_search_tools`)."
                }
            }),
        }
    }
}

fn classify(status: u16) -> (&'static str, &'static str) {
    match status {
        401 => (
            "UNAUTHORIZED",
            "Check PENNYLANE_API_KEY validity and scope.",
        ),
        403 => (
            "FORBIDDEN",
            "Token scope insufficient for this operation. See the operation's `scopes` field.",
        ),
        404 => (
            "NOT_FOUND",
            "Verify the resource id. Use the corresponding list_* operation to find a valid id.",
        ),
        422 => (
            "VALIDATION_FAILED",
            "Pennylane rejected the payload. Inspect upstream_body.errors for the offending fields.",
        ),
        429 => (
            "RATE_LIMITED",
            "Pennylane allows 25 requests / 5 seconds per token. Retry after the indicated delay.",
        ),
        s if (500..600).contains(&s) => (
            "UPSTREAM_ERROR",
            "Pennylane returned a server error. Retry later or check status.pennylane.com.",
        ),
        _ => ("UPSTREAM_ERROR", "Unexpected response from Pennylane."),
    }
}

fn truncate_body(body: &Value) -> Value {
    let s = body.to_string();
    if s.len() <= MAX_BODY_SNIPPET_BYTES {
        return body.clone();
    }
    let snippet = &s[..MAX_BODY_SNIPPET_BYTES];
    json!({
        "_truncated": true,
        "snippet": snippet,
        "original_size_bytes": s.len()
    })
}
