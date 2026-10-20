use bytes::Bytes;
use reqwest::header::{HeaderMap, HeaderName, HeaderValue, AUTHORIZATION, USER_AGENT};
use reqwest::multipart::{Form, Part};
use reqwest::{Body, Client, Method};
use serde_json::Value;
use std::fmt;
use std::time::Duration;
use tracing::{debug, info, warn};

use crate::config::scopes_imply_readonly;
use crate::error::ClientError;
use crate::redact::redact_bearer;

const REQUEST_TIMEOUT_SECS: u64 = 60;
const MAX_RETRY_AFTER_SECS: u64 = 60;
const BYTES_PER_UPLOAD_SECOND: u64 = 1_000_000;

/// A non-file part of a multipart form.
/// Plain data, so the form rebuilds for each attempt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FormPart {
    Text {
        name: String,
        value: String,
    },
    /// Already serialized, sent as `application/json`.
    Json {
        name: String,
        json: String,
    },
}

/// Everything a `multipart/form-data` request carries.
/// A reqwest form cannot be cloned.
/// Retries rebuild it from these parts.
/// File bytes are shared and never printed.
#[derive(Clone)]
pub struct MultipartBody {
    pub parts: Vec<FormPart>,
    pub file_field: String,
    pub file_name: String,
    pub content_type: &'static str,
    pub bytes: Bytes,
}

impl fmt::Debug for MultipartBody {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MultipartBody")
            .field("parts", &self.parts)
            .field("file_field", &self.file_field)
            .field("file_name", &self.file_name)
            .field("content_type", &self.content_type)
            .field("size_bytes", &self.bytes.len())
            .finish()
    }
}

impl MultipartBody {
    fn form(&self) -> Result<Form, ClientError> {
        let mut form = Form::new();
        for part in &self.parts {
            form = match part {
                FormPart::Text { name, value } => form.text(name.clone(), value.clone()),
                FormPart::Json { name, json } => form.part(
                    name.clone(),
                    Part::text(json.clone())
                        .mime_str("application/json")
                        .map_err(part_error)?,
                ),
            };
        }
        let file =
            Part::stream_with_length(Body::from(self.bytes.clone()), self.bytes.len() as u64)
                .file_name(self.file_name.clone())
                .mime_str(self.content_type)
                .map_err(part_error)?;
        Ok(form.part(self.file_field.clone(), file))
    }

    /// A POST is not safely repeatable.
    /// Slow uploads get time proportional to their size.
    /// A short deadline would turn a success into a duplicate.
    pub fn timeout(&self) -> Duration {
        let transfer_secs = (self.bytes.len() as u64).div_ceil(BYTES_PER_UPLOAD_SECOND);
        Duration::from_secs(REQUEST_TIMEOUT_SECS.saturating_add(transfer_secs))
    }
}

fn part_error(e: reqwest::Error) -> ClientError {
    ClientError::Network(format!("invalid multipart part: {}", e))
}

#[derive(Clone, Copy)]
enum RequestBody<'a> {
    None,
    Json(&'a Value),
    Multipart(&'a MultipartBody),
}

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
        let body = body.as_ref().map_or(RequestBody::None, RequestBody::Json);
        self.execute(method, path, query, body).await
    }

    /// Send a `multipart/form-data` request.
    /// Any network failure means the upload was interrupted.
    /// The file may have arrived: a retry can duplicate it.
    pub async fn call_multipart(
        &self,
        method: &str,
        path: &str,
        query: &[(String, String)],
        body: &MultipartBody,
    ) -> Result<Value, ClientError> {
        self.execute(method, path, query, RequestBody::Multipart(body))
            .await
            .map_err(|e| match e {
                network @ ClientError::Network(_) => {
                    ClientError::UploadInterrupted(Box::new(network))
                }
                other => other,
            })
    }

    async fn execute(
        &self,
        method: &str,
        path: &str,
        query: &[(String, String)],
        body: RequestBody<'_>,
    ) -> Result<Value, ClientError> {
        let url = format!("{}{}", self.base_url, path);
        let http_method = method
            .parse::<Method>()
            .map_err(|e| ClientError::InvalidArgs {
                operation_id: path.to_string(),
                message: format!("unsupported HTTP method `{}`: {}", method, e),
            })?;

        let started = std::time::Instant::now();

        let mut response = self.send_once(&http_method, &url, query, body).await?;
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
            response = self.send_once(&http_method, &url, query, body).await?;
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
            required_scopes: Vec::new(),
        })
    }

    async fn send_once(
        &self,
        method: &Method,
        url: &str,
        query: &[(String, String)],
        body: RequestBody<'_>,
    ) -> Result<reqwest::Response, ClientError> {
        let mut req = self.http.request(method.clone(), url);
        if !query.is_empty() {
            req = req.query(query);
        }
        match body {
            RequestBody::None => {}
            RequestBody::Json(b) => req = req.json(b),
            RequestBody::Multipart(m) => req = req.multipart(m.form()?).timeout(m.timeout()),
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

#[cfg(test)]
mod tests {
    use super::*;
    use http_body_util::BodyExt;
    use reqwest::header::CONTENT_TYPE;

    fn body(bytes: &'static [u8], parts: Vec<FormPart>) -> MultipartBody {
        MultipartBody {
            parts,
            file_field: "file".to_string(),
            file_name: "Facture été août.pdf".to_string(),
            content_type: "application/pdf",
            bytes: Bytes::from_static(bytes),
        }
    }

    /// Renders the wire bytes without sending anything.
    async fn rendered(body: &MultipartBody) -> (String, Vec<u8>) {
        let client = Client::builder().build().expect("client");
        let mut request = client
            .post("http://127.0.0.1:9/")
            .multipart(body.form().expect("form"))
            .build()
            .expect("request");
        let content_type = request.headers()[CONTENT_TYPE]
            .to_str()
            .expect("content type")
            .to_string();
        let request_body = request.body_mut().take().expect("body");
        let bytes = request_body.collect().await.expect("collect").to_bytes();
        (content_type, bytes.to_vec())
    }

    fn contains(haystack: &[u8], needle: &str) -> bool {
        haystack
            .windows(needle.len())
            .any(|window| window == needle.as_bytes())
    }

    #[tokio::test]
    async fn accented_filename_text_part_goes_out_as_intact_utf8() {
        let form = body(
            b"%PDF-1.4 content",
            vec![FormPart::Text {
                name: "filename".to_string(),
                value: "Facture été août.pdf".to_string(),
            }],
        );
        let (content_type, wire) = rendered(&form).await;
        assert!(content_type.starts_with("multipart/form-data; boundary="));
        assert!(contains(
            &wire,
            "name=\"filename\"\r\n\r\nFacture été août.pdf\r\n"
        ));
        assert!(contains(
            &wire,
            "name=\"file\"; filename=\"Facture été août.pdf\"\r\nContent-Type: application/pdf"
        ));
        assert!(contains(&wire, "\r\n\r\n%PDF-1.4 content\r\n"));
    }

    #[tokio::test]
    async fn json_part_is_sent_as_application_json_without_a_filename() {
        let form = body(
            b"%PDF-1.4",
            vec![FormPart::Json {
                name: "invoice_options".to_string(),
                json: "{\"supplier_id\":12}".to_string(),
            }],
        );
        let (_, wire) = rendered(&form).await;
        assert!(contains(
            &wire,
            "name=\"invoice_options\"\r\nContent-Type: application/json\r\n\r\n{\"supplier_id\":12}\r\n"
        ));
    }

    #[tokio::test]
    async fn the_form_is_rebuilt_identically_for_a_retry() {
        let form = body(b"%PDF-1.4 retry", Vec::new());
        let (_, first) = rendered(&form).await;
        let (_, second) = rendered(&form).await;
        let strip_boundary = |wire: &[u8]| {
            let text = String::from_utf8_lossy(wire).into_owned();
            let boundary = text.lines().next().expect("boundary line").to_string();
            text.replace(&boundary, "--B")
        };
        assert_eq!(strip_boundary(&first), strip_boundary(&second));
    }

    #[test]
    fn upload_deadline_is_a_minute_plus_a_second_per_megabyte() {
        let sized = |len: usize| MultipartBody {
            bytes: Bytes::from(vec![0u8; len]),
            ..body(b"", Vec::new())
        };
        assert_eq!(sized(0).timeout(), Duration::from_secs(60));
        assert_eq!(sized(1).timeout(), Duration::from_secs(61));
        assert_eq!(sized(1_000_000).timeout(), Duration::from_secs(61));
        assert_eq!(sized(1_000_001).timeout(), Duration::from_secs(62));
        assert_eq!(sized(100_000_000).timeout(), Duration::from_secs(160));
    }

    #[test]
    fn debug_output_never_contains_the_file_bytes() {
        let form = body(b"confidential payroll data", Vec::new());
        let printed = format!("{:?}", form);
        assert!(!printed.contains("confidential"), "{printed}");
        assert!(printed.contains("size_bytes: 25"), "{printed}");
    }

    #[tokio::test]
    async fn unreachable_server_is_reported_as_an_interrupted_upload() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
        let port = listener.local_addr().expect("addr").port();
        drop(listener);
        let client = PennylaneClient::new(
            "token-0123456789",
            &format!("http://127.0.0.1:{port}"),
            false,
        )
        .expect("client");
        let error = client
            .call_multipart(
                "POST",
                "/api/external/v2/file_attachments",
                &[],
                &body(b"%PDF-1.4", Vec::new()),
            )
            .await
            .expect_err("nothing listens on that port");
        assert!(
            matches!(error, ClientError::UploadInterrupted(_)),
            "{error:?}"
        );
        assert_eq!(error.to_mcp_value()["error"]["code"], "NETWORK_ERROR");
    }
}
