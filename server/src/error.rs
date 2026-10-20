use serde_json::{json, Value};
use std::time::Duration;

const MAX_BODY_SNIPPET_BYTES: usize = 2_048;
const SUGGESTION_KEY: &str = "suggestion";

#[derive(thiserror::Error, Debug)]
pub enum ClientError {
    #[error("network error: {0}")]
    Network(String),
    #[error("network error during a file upload: {0}")]
    UploadInterrupted(#[source] Box<ClientError>),
    #[error("{limit} uploads are already running and none finished within {waited:?}")]
    UploadsBusy { limit: usize, waited: Duration },
    #[error("HTTP {status}")]
    Http {
        status: u16,
        body: Value,
        retry_after_seconds: Option<u64>,
        required_scopes: Vec<String>,
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
    /// Attach the operation's required scopes for 403 errors.
    #[must_use]
    pub fn with_required_scopes(self, scopes: &[&str]) -> Self {
        match self {
            ClientError::Http {
                status,
                body,
                retry_after_seconds,
                ..
            } => ClientError::Http {
                status,
                body,
                retry_after_seconds,
                required_scopes: scopes.iter().map(|s| s.to_string()).collect(),
            },
            other => other,
        }
    }

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
            ClientError::UploadInterrupted(cause) => upload_interrupted_value(cause),
            ClientError::UploadsBusy { .. } => uploads_busy_value(self),
            ClientError::Http {
                status,
                body,
                retry_after_seconds,
                required_scopes,
            } => http_value(*status, body, *retry_after_seconds, required_scopes),
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

fn upload_interrupted_value(cause: &ClientError) -> Value {
    let detail = match cause {
        ClientError::Network(message) => message.clone(),
        other => other.to_string(),
    };
    json!({
        "error": {
            "code": "NETWORK_ERROR",
            "message": format!("{detail}. The file may have been received by Pennylane; check before sending it again."),
            SUGGESTION_KEY: "Do not resend blindly: look for the attachment first (a retry can create a duplicate), and resend only if it is missing."
        }
    })
}

fn uploads_busy_value(busy: &ClientError) -> Value {
    json!({
        "error": {
            "code": "UPLOADS_BUSY",
            "message": busy.to_string(),
            SUGGESTION_KEY: "Wait for a running upload to finish, then call again."
        }
    })
}

fn http_value(
    status: u16,
    body: &Value,
    retry_after_seconds: Option<u64>,
    required_scopes: &[String],
) -> Value {
    let (code, suggestion) = classify(status);
    let names_scopes = status == 403 && !required_scopes.is_empty();
    let suggestion = if names_scopes {
        scope_suggestion(required_scopes)
    } else {
        suggestion.to_string()
    };
    let mut err = serde_json::Map::new();
    err.insert("code".into(), json!(code));
    err.insert(
        "message".into(),
        json!(format!("Pennylane responded {}", status)),
    );
    err.insert("status".into(), json!(status));
    err.insert(SUGGESTION_KEY.into(), json!(suggestion));
    if names_scopes {
        err.insert("required_scopes".into(), json!(required_scopes));
    }
    err.insert("upstream_body".into(), truncate_body(body));
    if let Some(secs) = retry_after_seconds {
        err.insert("retry_after_seconds".into(), json!(secs));
    }
    json!({ "error": err })
}

fn scope_suggestion(required_scopes: &[String]) -> String {
    format!(
        "Token scope insufficient for this operation, which requires: {}. Call `getMe` to see the scopes of the token.",
        required_scopes.join(", ")
    )
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

#[cfg(test)]
mod tests {
    use super::*;

    fn forbidden() -> ClientError {
        ClientError::Http {
            status: 403,
            body: json!({"error": "forbidden"}),
            retry_after_seconds: None,
            required_scopes: Vec::new(),
        }
    }

    #[test]
    fn forbidden_upload_names_the_scopes_the_operation_requires() {
        let rendered = forbidden()
            .with_required_scopes(&["file_attachments:all"])
            .to_mcp_value();
        assert_eq!(rendered["error"]["code"], "FORBIDDEN");
        assert_eq!(
            rendered["error"]["required_scopes"],
            json!(["file_attachments:all"])
        );
        let suggestion = rendered["error"]["suggestion"]
            .as_str()
            .expect("suggestion");
        assert!(suggestion.contains("file_attachments:all"), "{suggestion}");
        assert!(suggestion.contains("getMe"), "{suggestion}");
    }

    #[test]
    fn forbidden_without_scopes_keeps_the_generic_hint() {
        let rendered = forbidden().to_mcp_value();
        assert!(rendered["error"].get("required_scopes").is_none());
        assert_eq!(
            rendered["error"]["suggestion"],
            "Token scope insufficient for this operation. See the operation's `scopes` field."
        );
    }

    #[test]
    fn scopes_are_attached_to_http_errors_only() {
        let other = ClientError::UnknownTool("x".to_string()).with_required_scopes(&["a:all"]);
        assert!(matches!(other, ClientError::UnknownTool(_)));
        let not_forbidden = ClientError::Http {
            status: 422,
            body: Value::Null,
            retry_after_seconds: None,
            required_scopes: Vec::new(),
        }
        .with_required_scopes(&["a:all"])
        .to_mcp_value();
        assert!(not_forbidden["error"].get("required_scopes").is_none());
    }

    #[test]
    fn busy_uploads_say_how_many_run_and_how_long_the_wait_was() {
        let busy = ClientError::UploadsBusy {
            limit: 4,
            waited: Duration::from_secs(30),
        };
        let rendered = busy.to_mcp_value();
        assert_eq!(rendered["error"]["code"], "UPLOADS_BUSY");
        let message = rendered["error"]["message"].as_str().expect("message");
        assert!(
            message.contains("4 uploads are already running"),
            "{message}"
        );
        assert!(message.contains("none finished within 30s"), "{message}");
        let suggestion = rendered["error"]["suggestion"]
            .as_str()
            .expect("suggestion");
        assert!(suggestion.contains("then call again"), "{suggestion}");
    }

    #[test]
    fn interrupted_upload_warns_that_the_file_may_have_arrived() {
        let cause = ClientError::Network("operation timed out".to_string());
        let rendered = ClientError::UploadInterrupted(Box::new(cause)).to_mcp_value();
        assert_eq!(rendered["error"]["code"], "NETWORK_ERROR");
        let message = rendered["error"]["message"].as_str().expect("message");
        assert!(message.starts_with("operation timed out"), "{message}");
        assert!(
            message.contains("may have been received by Pennylane"),
            "{message}"
        );
        let suggestion = rendered["error"]["suggestion"]
            .as_str()
            .expect("suggestion");
        assert!(suggestion.contains("Do not resend blindly"), "{suggestion}");
        assert!(!suggestion.contains("and retry"), "{suggestion}");
    }
}
