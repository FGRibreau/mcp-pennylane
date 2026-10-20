use anyhow::Result;
use rmcp::handler::server::ServerHandler;
use rmcp::model::{
    CallToolRequestParam, CallToolResult, Content, Implementation, ListToolsResult,
    PaginatedRequestParam, ProtocolVersion, ServerCapabilities, ServerInfo, Tool,
};
use rmcp::service::{RequestContext, RoleServer};
use rmcp::ErrorData;
use serde_json::{json, Map, Value};
use std::borrow::Cow;
use std::sync::Arc;
use tracing::info;

use crate::client::PennylaneClient;
use crate::config::Config;
use crate::error::ClientError;
use crate::generated::{GeneratedTool, GENERATED_TOOLS, SPEC_VERSION};
use crate::tools::{
    execute_tool_schema, lookup_tool, search, search_tool_schema, split_params, visible_tools,
    META_EXECUTE_TOOL, META_SEARCH_TOOL,
};
use crate::upload::{audit, prepare, MAX_UPLOAD_BYTES};

#[derive(Clone)]
pub struct PennylaneService {
    cfg: Config,
    client: Arc<PennylaneClient>,
    visible: Arc<Vec<&'static GeneratedTool>>,
}

impl PennylaneService {
    pub fn new(cfg: Config) -> Result<Self> {
        let client = PennylaneClient::new(&cfg.token, &cfg.base_url, cfg.api_2026)
            .map_err(|e| anyhow::anyhow!("{}", e))?;
        let visible = visible_tools(cfg.readonly);
        Ok(Self {
            cfg,
            client: Arc::new(client),
            visible: Arc::new(visible),
        })
    }

    pub fn exposed_tool_count(&self) -> usize {
        self.visible.len() + 2
    }

    fn instructions(&self) -> String {
        format!(
            "Pennylane Company API v{spec} — env={env} mode={mode} ({source}) api_2026={a2026}\n\
             Token: {token}\n\
             Base URL: {base}\n\n\
             Direct tools: {direct}. Long tail: use `pennylane_search_tools(query)` to discover, \
             then `pennylane_execute(tool_name, params)` to invoke any of the {total} operations.\n\
             Pagination: list_* GETs return cursor / has_more / next_cursor — pass next_cursor in \
             the `cursor` parameter to fetch the next page.\n\
             Filters: pass `filter` or `filters` as JSON arrays of {{field, operator, value}}.",
            spec = SPEC_VERSION,
            env = self.cfg.environment,
            mode = if self.cfg.readonly {
                "readonly"
            } else {
                "read+write"
            },
            source = self.cfg.readonly_source,
            a2026 = self.cfg.api_2026,
            token = self.client.token_redacted(),
            base = self.cfg.base_url,
            direct = self.visible.len(),
            total = GENERATED_TOOLS.len(),
        )
    }

    fn tool_to_mcp(&self, t: &GeneratedTool) -> Tool {
        let schema: Value =
            serde_json::from_str(t.input_schema_json).unwrap_or(Value::Object(Map::new()));
        let schema_obj = match schema {
            Value::Object(map) => map,
            _ => Map::new(),
        };
        Tool {
            name: Cow::Borrowed(t.name),
            description: Some(Cow::Owned(format!("[{}] {}", t.method, t.description))),
            input_schema: Arc::new(schema_obj),
            annotations: None,
            icons: None,
            meta: None,
            output_schema: None,
            title: None,
        }
    }

    fn meta_search(&self) -> Tool {
        Tool {
            name: Cow::Borrowed(META_SEARCH_TOOL),
            description: Some(Cow::Borrowed(
                "Search the full Pennylane API surface (~163 ops). \
                 Returns matching operationIds with their input schema. \
                 Pair with `pennylane_execute` to invoke a result.",
            )),
            input_schema: Arc::new(value_to_object(search_tool_schema())),
            annotations: None,
            icons: None,
            meta: None,
            output_schema: None,
            title: None,
        }
    }

    fn meta_execute(&self) -> Tool {
        Tool {
            name: Cow::Borrowed(META_EXECUTE_TOOL),
            description: Some(Cow::Borrowed(
                "Invoke any Pennylane operation by its operationId. \
                 `params` is a flat object containing path params, query params, and body fields; \
                 the dispatcher splits them based on the OpenAPI path template.",
            )),
            input_schema: Arc::new(value_to_object(execute_tool_schema())),
            annotations: None,
            icons: None,
            meta: None,
            output_schema: None,
            title: None,
        }
    }

    async fn dispatch(&self, tool_name: &str, params: &Map<String, Value>) -> Value {
        let result = self.dispatch_inner(tool_name, params).await;
        match result {
            Ok(v) => v,
            Err(e) => e.to_mcp_value(),
        }
    }

    async fn dispatch_inner(
        &self,
        tool_name: &str,
        params: &Map<String, Value>,
    ) -> Result<Value, ClientError> {
        let tool = lookup_tool(tool_name)
            .ok_or_else(|| ClientError::UnknownTool(tool_name.to_string()))?;

        if self.cfg.readonly && tool.method != "GET" {
            return Err(ClientError::Readonly {
                operation_id: tool.name.to_string(),
                method: tool.method.to_string(),
            });
        }

        let (path, query, body) = split_params(tool, params)?;
        let mut response = if tool.multipart {
            let upload = prepare(tool, body, &self.cfg.upload_dirs, MAX_UPLOAD_BYTES).await?;
            info!(
                operation_id = tool.name,
                path = %upload.path.display(),
                filename = %upload.body.file_name,
                size_bytes = upload.body.bytes.len(),
                "uploading file"
            );
            let mut response = self
                .client
                .call_multipart(tool.method, &path, &query, &upload.body)
                .await
                .map_err(|e| e.with_required_scopes(tool.scopes))?;
            augment_upload(&mut response, audit(&upload));
            response
        } else {
            self.client.call(tool.method, &path, &query, body).await?
        };

        if tool.name == "getMe" {
            augment_get_me(&mut response, &self.cfg);
        }

        Ok(response)
    }
}

fn value_to_object(v: Value) -> Map<String, Value> {
    match v {
        Value::Object(m) => m,
        _ => Map::new(),
    }
}

fn augment_get_me(response: &mut Value, cfg: &Config) {
    if let Value::Object(map) = response {
        map.insert(
            "_mcp_pennylane".to_string(),
            json!({
                "server_version": env!("CARGO_PKG_VERSION"),
                "spec_version": SPEC_VERSION,
                "env": cfg.environment.to_string(),
                "readonly": cfg.readonly,
                "readonly_source": cfg.readonly_source.to_string(),
                "api_2026": cfg.api_2026,
            }),
        );
    }
}

fn augment_upload(response: &mut Value, uploaded_file: Value) {
    if let Value::Object(map) = response {
        map.insert(
            "_mcp_pennylane".to_string(),
            json!({ "uploaded_file": uploaded_file }),
        );
    }
}

impl ServerHandler for PennylaneService {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            protocol_version: ProtocolVersion::V_2024_11_05,
            capabilities: ServerCapabilities::builder().enable_tools().build(),
            server_info: Implementation {
                name: "mcp-pennylane".to_string(),
                version: env!("CARGO_PKG_VERSION").to_string(),
                icons: None,
                title: None,
                website_url: None,
            },
            instructions: Some(self.instructions()),
        }
    }

    async fn list_tools(
        &self,
        _request: Option<PaginatedRequestParam>,
        _ctx: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, ErrorData> {
        let mut tools: Vec<Tool> = self.visible.iter().map(|t| self.tool_to_mcp(t)).collect();
        tools.push(self.meta_search());
        tools.push(self.meta_execute());
        Ok(ListToolsResult {
            tools,
            next_cursor: None,
            meta: None,
        })
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParam,
        _ctx: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, ErrorData> {
        let name = request.name.as_ref();
        let args: Map<String, Value> = request.arguments.unwrap_or_default().into_iter().collect();

        let value = match name {
            n if n == META_SEARCH_TOOL => {
                let query = args
                    .get("query")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let limit = args
                    .get("limit")
                    .and_then(|v| v.as_u64())
                    .map(|n| n as usize)
                    .unwrap_or(10);
                search(&query, limit, self.cfg.readonly)
            }
            n if n == META_EXECUTE_TOOL => {
                let tool_name = args.get("tool_name").and_then(|v| v.as_str()).unwrap_or("");
                let params = args
                    .get("params")
                    .and_then(|v| v.as_object())
                    .cloned()
                    .unwrap_or_default();
                self.dispatch(tool_name, &params).await
            }
            other => self.dispatch(other, &args).await,
        };

        let body = serde_json::to_string_pretty(&value).unwrap_or_else(|_| value.to_string());
        let is_error = value.get("error").and_then(|e| e.as_object()).is_some();

        Ok(CallToolResult {
            content: vec![Content::text(body)],
            is_error: Some(is_error),
            meta: None,
            structured_content: None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{Environment, ReadonlySource, Transport};

    const PIXEL_PNG: &[u8] = include_bytes!("../tests/fixtures/pixel.png");

    fn service(
        is_readonly: bool,
        base_url: &str,
        upload_dirs: Vec<std::path::PathBuf>,
    ) -> PennylaneService {
        PennylaneService::new(Config {
            token: "token-0123456789".to_string(),
            base_url: base_url.to_string(),
            readonly: is_readonly,
            readonly_source: ReadonlySource::Explicit,
            environment: Environment::Sandbox,
            api_2026: false,
            transport: Transport::Stdio,
            http_host: "127.0.0.1".to_string(),
            http_port: 0,
            upload_dirs,
        })
        .expect("service")
    }

    /// A loopback address nothing listens on.
    /// Requests that reach the network fail fast.
    /// Guards stop the others before that.
    fn closed_port_url() -> String {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
        let port = listener.local_addr().expect("addr").port();
        format!("http://127.0.0.1:{port}")
    }

    fn params(value: Value) -> Map<String, Value> {
        value.as_object().cloned().expect("params are an object")
    }

    struct TempFile(std::path::PathBuf);

    impl TempFile {
        fn png(label: &str) -> Self {
            let path = std::env::temp_dir().join(format!(
                "mcp-pennylane-server-{}-{}.png",
                label,
                std::process::id()
            ));
            std::fs::write(&path, PIXEL_PNG).expect("write fixture copy");
            TempFile(path)
        }

        fn path(&self) -> String {
            self.0.display().to_string()
        }
    }

    impl Drop for TempFile {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.0);
        }
    }

    #[tokio::test]
    async fn read_only_mode_refuses_every_multipart_operation_without_reading_the_file() {
        let service = service(true, &closed_port_url(), Vec::new());
        let multipart: Vec<&str> = GENERATED_TOOLS
            .iter()
            .filter(|t| t.multipart)
            .map(|t| t.name)
            .collect();
        assert_eq!(multipart.len(), 8);
        for name in multipart {
            let outcome = service
                .dispatch_inner(
                    name,
                    &params(json!({"file": "/nonexistent/never-read.pdf"})),
                )
                .await;
            assert!(
                matches!(outcome, Err(ClientError::Readonly { .. })),
                "`{name}` was not refused as read-only: {outcome:?}"
            );
        }
    }

    #[tokio::test]
    async fn guards_stop_a_request_before_it_reaches_the_network() {
        let service = service(false, &closed_port_url(), Vec::new());
        let outcome = service
            .dispatch_inner(
                "postFileAttachments",
                &params(json!({"file": "relative.pdf"})),
            )
            .await;
        assert!(
            matches!(outcome, Err(ClientError::InvalidArgs { .. })),
            "{outcome:?}"
        );
    }

    #[tokio::test]
    async fn a_valid_upload_reaches_the_network_layer() {
        let service = service(false, &closed_port_url(), Vec::new());
        let file = TempFile::png("valid");
        let outcome = service
            .dispatch_inner("postFileAttachments", &params(json!({"file": file.path()})))
            .await;
        assert!(
            matches!(outcome, Err(ClientError::UploadInterrupted(_))),
            "{outcome:?}"
        );
    }

    #[tokio::test]
    async fn configured_upload_directories_confine_the_service() {
        let file = TempFile::png("confined");
        let only_allowed = std::env::temp_dir().join("mcp-pennylane-allowed-and-empty");
        let service = service(false, &closed_port_url(), vec![only_allowed]);
        let outcome = service
            .dispatch_inner("postFileAttachments", &params(json!({"file": file.path()})))
            .await;
        match outcome {
            Err(ClientError::InvalidArgs { message, .. }) => {
                assert!(
                    message.contains("outside the directories allowed"),
                    "{message}"
                )
            }
            other => panic!("expected a refusal, got {other:?}"),
        }
    }
}
