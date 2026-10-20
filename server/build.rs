//! Build-time OpenAPI parser → emits `src/generated.rs` with the full Pennylane
//! tool catalog as static constants. Fail-fast on:
//! - missing or malformed `openapi/accounting.json`
//! - any operationId in `ESSENTIALS` that no longer exists in the spec
//! - any tool name >= 64 characters (Claude Code rejects those silently)

use indexmap::IndexMap;
use serde::Deserialize;
use std::collections::BTreeSet;
use std::env;
use std::fs;
use std::path::PathBuf;

const SPEC_RELATIVE_PATH: &str = "openapi/accounting.json";
const MAX_TOOL_NAME_LENGTH: usize = 64;
const JSON_MEDIA_TYPE: &str = "application/json";
const MULTIPART_MEDIA_TYPE: &str = "multipart/form-data";
const PROPERTIES_KEY: &str = "properties";
const DESCRIPTION_KEY: &str = "description";

/// Prefixed to binary field descriptions.
/// The caller passes a path, not the bytes.
const LOCAL_FILE_HINT: &str = "Absolute path of a local file on the machine running this server \
     (`~` and relative paths are refused; symbolic links are resolved).";

/// Operations exposed directly in `tools/list`. Everything else is reachable
/// via the `pennylane_search_tools` + `pennylane_execute` meta-tools.
///
/// The build script panics if any of these is missing from the vendored spec —
/// forces the maintainer to react when Pennylane renames an endpoint.
const ESSENTIALS: &[&str] = &[
    // Customers
    "getCustomers",
    "getCustomer",
    "getCustomerContacts",
    "getCustomerCategories",
    "postCompanyCustomer",
    "postIndividualCustomer",
    "putCompanyCustomer",
    "putIndividualCustomer",
    // Suppliers
    "getSuppliers",
    "getSupplier",
    "getSupplierCategories",
    "postSupplier",
    "putSupplier",
    // Customer invoices
    "getCustomerInvoices",
    "getCustomerInvoice",
    "getCustomerInvoiceInvoiceLines",
    "getCustomerInvoicePayments",
    "getCustomerInvoiceMatchedTransactions",
    "postCustomerInvoices",
    "updateCustomerInvoice",
    "finalizeCustomerInvoice",
    "markAsPaidCustomerInvoice",
    "sendByEmailCustomerInvoice",
    // Supplier invoices
    "getSupplierInvoices",
    "getSupplierInvoice",
    "getSupplierInvoiceLines",
    "putSupplierInvoice",
    "updateSupplierInvoicePaymentStatus",
    "createSupplierInvoiceEInvoiceImport",
    // Products
    "getProducts",
    "getProduct",
    "postProducts",
    "putProduct",
    // Quotes
    "listQuotes",
    "getQuote",
    "postQuotes",
    "sendByEmailQuote",
    // Banking
    "getBankAccounts",
    "getTransactions",
    "getTransaction",
    // Accounting / ledger
    "getJournals",
    "getLedgerAccounts",
    "getLedgerEntries",
    "getLedgerEntry",
    "getLedgerEntryLines",
    "getTrialBalance",
    "postLedgerEntries",
    "postLedgerEntryLinesLetter",
    // Analytics
    "getCategories",
    "getCategoryGroups",
    // Exports
    "exportFec",
    "getFecExport",
    // File attachments (legacy + 2026)
    "postFileAttachments",
    "postLedgerAttachments",
    // Changelogs (audit / sync)
    "getCustomerInvoicesChanges",
    "getSupplierInvoicesChanges",
    "getCustomerChanges",
    "getSupplierChanges",
    "getProductChanges",
    "getLedgerEntryLineChanges",
    "getTransactionChanges",
    "getQuoteChanges",
    // GoCardless mandates
    "getGocardlessMandates",
    "getGocardlessMandate",
    "postGocardlessMandateMailRequests",
    "postGocardlessMandateCancellations",
    "postGocardlessMandateAssociations",
    // SEPA mandates
    "getSepaMandates",
    "getSepaMandate",
    "postSepaMandates",
    "putSepaMandate",
    "deleteSepaMandate",
    // Misc
    "getMe",
    "company-fiscal-years",
];

#[derive(Debug, Deserialize)]
struct OpenApiSpec {
    openapi: String,
    info: Info,
    paths: IndexMap<String, IndexMap<String, serde_json::Value>>,
}

#[derive(Debug, Deserialize)]
struct Info {
    version: String,
}

#[derive(Debug, Deserialize)]
struct Operation {
    #[serde(rename = "operationId")]
    operation_id: Option<String>,
    summary: Option<String>,
    description: Option<String>,
    #[serde(default)]
    parameters: Vec<Parameter>,
    #[serde(rename = "requestBody")]
    request_body: Option<RequestBody>,
    #[serde(default)]
    security: Vec<IndexMap<String, Vec<String>>>,
}

#[derive(Debug, Deserialize)]
struct Parameter {
    name: String,
    #[serde(rename = "in")]
    location: String,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    required: bool,
    #[serde(default)]
    schema: serde_json::Value,
}

#[derive(Debug, Deserialize)]
struct RequestBody {
    #[serde(default)]
    required: bool,
    content: IndexMap<String, MediaType>,
}

impl RequestBody {
    /// JSON wins when an operation offers both encodings.
    fn is_multipart(&self) -> bool {
        !self.content.contains_key(JSON_MEDIA_TYPE)
            && self.content.contains_key(MULTIPART_MEDIA_TYPE)
    }

    fn media(&self) -> Option<&MediaType> {
        self.content
            .get(JSON_MEDIA_TYPE)
            .or_else(|| self.content.get(MULTIPART_MEDIA_TYPE))
    }
}

#[derive(Debug, Deserialize)]
struct MediaType {
    schema: serde_json::Value,
}

fn main() {
    let cargo_manifest_dir =
        env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR is set by cargo");
    let spec_path = PathBuf::from(&cargo_manifest_dir).join(SPEC_RELATIVE_PATH);

    println!("cargo:rerun-if-changed={}", spec_path.display());
    println!("cargo:rerun-if-changed=build.rs");

    let raw = fs::read_to_string(&spec_path).unwrap_or_else(|e| {
        panic!(
            "failed to read {}: {}\n\
             Run `cargo run -p refresh-openapi` to download a fresh copy.",
            spec_path.display(),
            e
        )
    });

    let spec: OpenApiSpec = serde_json::from_str(&raw).unwrap_or_else(|e| {
        panic!(
            "failed to parse {} as OpenAPI 3.0+ JSON: {}\n\
             Run `cargo run -p refresh-openapi` to refresh the vendored spec.",
            spec_path.display(),
            e
        )
    });

    if !spec.openapi.starts_with("3.") {
        panic!(
            "unsupported OpenAPI version `{}` (expected 3.x) in {}",
            spec.openapi,
            spec_path.display()
        );
    }

    let tools = extract_tools(&spec);
    validate_tool_name_lengths(&tools);
    validate_essentials_present(&tools);

    let code = render_generated_code(&tools, &spec.info.version);
    let out_dir = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR is set by cargo"));
    let out_path = out_dir.join("generated.rs");
    fs::write(&out_path, code)
        .unwrap_or_else(|e| panic!("failed to write {}: {}", out_path.display(), e));
}

#[derive(Debug)]
struct GeneratedTool {
    name: String,
    description: String,
    method: String,
    path_template: String,
    input_schema_json: String,
    scopes: Vec<String>,
    multipart: bool,
    binary_fields: Vec<String>,
}

fn extract_tools(spec: &OpenApiSpec) -> Vec<GeneratedTool> {
    let mut tools = Vec::new();

    for (path, methods) in &spec.paths {
        for (method, op_value) in methods {
            let method_lower = method.to_lowercase();
            if !matches!(
                method_lower.as_str(),
                "get" | "post" | "put" | "patch" | "delete"
            ) {
                continue;
            }

            let op: Operation = match serde_json::from_value(op_value.clone()) {
                Ok(o) => o,
                Err(_) => continue,
            };

            let Some(operation_id) = op.operation_id.clone() else {
                continue;
            };

            let description = op
                .summary
                .clone()
                .or(op.description.clone())
                .unwrap_or_else(|| format!("{} {}", method.to_uppercase(), path));

            let input_schema = build_input_schema(&op, path);
            let scopes = extract_scopes(&op);
            let multipart = op.request_body.as_ref().is_some_and(|b| b.is_multipart());
            let binary_fields = if multipart {
                binary_fields(&op, &operation_id)
            } else {
                Vec::new()
            };

            tools.push(GeneratedTool {
                name: operation_id,
                description,
                method: method_lower.to_uppercase(),
                path_template: path.clone(),
                input_schema_json: serde_json::to_string(&input_schema)
                    .unwrap_or_else(|_| "{\"type\":\"object\",\"properties\":{}}".to_string()),
                scopes,
                multipart,
                binary_fields,
            });
        }
    }

    tools
}

fn build_input_schema(op: &Operation, path: &str) -> serde_json::Value {
    let mut properties = serde_json::Map::new();
    let mut required: Vec<serde_json::Value> = Vec::new();

    for placeholder in path_placeholders(path) {
        let mut prop = serde_json::Map::new();
        prop.insert(
            "type".to_string(),
            serde_json::Value::String("string".to_string()),
        );
        prop.insert(
            "description".to_string(),
            serde_json::Value::String(format!("Path parameter `{}`", placeholder)),
        );
        properties.insert(placeholder.clone(), serde_json::Value::Object(prop));
        required.push(serde_json::Value::String(placeholder));
    }

    for p in &op.parameters {
        if p.location != "query" && p.location != "path" {
            continue;
        }
        let mut prop = match p.schema.clone() {
            serde_json::Value::Object(map) => map,
            _ => serde_json::Map::new(),
        };
        if let Some(d) = p.description.clone() {
            prop.entry("description".to_string())
                .or_insert(serde_json::Value::String(d));
        }
        properties.insert(p.name.clone(), serde_json::Value::Object(prop));
        if p.required {
            let val = serde_json::Value::String(p.name.clone());
            if !required.contains(&val) {
                required.push(val);
            }
        }
    }

    if let Some(body) = &op.request_body {
        merge_body_schema(body, &mut properties, &mut required);
    }

    let mut schema = serde_json::Map::new();
    schema.insert(
        "type".to_string(),
        serde_json::Value::String("object".to_string()),
    );
    schema.insert(
        "properties".to_string(),
        serde_json::Value::Object(properties),
    );
    if !required.is_empty() {
        schema.insert("required".to_string(), serde_json::Value::Array(required));
    }
    serde_json::Value::Object(schema)
}

/// Merge the request body schema into the input schema.
fn merge_body_schema(
    body: &RequestBody,
    properties: &mut serde_json::Map<String, serde_json::Value>,
    required: &mut Vec<serde_json::Value>,
) {
    let Some(media) = body.media() else { return };
    let serde_json::Value::Object(body_obj) = &media.schema else {
        return;
    };
    if let Some(serde_json::Value::Object(body_props)) = body_obj.get(PROPERTIES_KEY) {
        for (k, v) in body_props {
            properties.insert(k.clone(), body_property(body, v));
        }
    }
    if !body.required {
        return;
    }
    let Some(serde_json::Value::Array(body_required)) = body_obj.get("required") else {
        return;
    };
    for item in body_required {
        if !required.contains(item) {
            required.push(item.clone());
        }
    }
}

fn body_property(body: &RequestBody, property: &serde_json::Value) -> serde_json::Value {
    if body.is_multipart() && is_binary(property) {
        return local_file_property(property);
    }
    property.clone()
}

fn is_binary(property: &serde_json::Value) -> bool {
    property.get("format").and_then(|f| f.as_str()) == Some("binary")
}

fn local_file_property(original: &serde_json::Value) -> serde_json::Value {
    let original_description = original
        .get(DESCRIPTION_KEY)
        .and_then(|d| d.as_str())
        .unwrap_or("");
    serde_json::json!({
        "type": "string",
        DESCRIPTION_KEY: format!("{LOCAL_FILE_HINT} {original_description}").trim_end(),
    })
}

/// Names of the binary fields of a multipart body.
/// Exactly one is required, otherwise the build fails.
fn binary_fields(op: &Operation, operation_id: &str) -> Vec<String> {
    let properties = op
        .request_body
        .as_ref()
        .and_then(|body| body.media())
        .and_then(|media| media.schema.get(PROPERTIES_KEY))
        .and_then(|props| props.as_object());
    let fields: Vec<String> = properties
        .into_iter()
        .flatten()
        .filter(|(_, schema)| is_binary(schema))
        .map(|(name, _)| name.clone())
        .collect();

    assert!(
        fields.len() == 1,
        "multipart operation `{}` has {} binary field(s) ({:?}); the upload module sends exactly one file. \
         Extend `server/src/upload.rs` before accepting this spec.",
        operation_id,
        fields.len(),
        fields
    );
    fields
}

fn path_placeholders(path: &str) -> Vec<String> {
    let mut out = Vec::new();
    let bytes = path.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'{' {
            if let Some(end) = path[i + 1..].find('}') {
                out.push(path[i + 1..i + 1 + end].to_string());
                i += end + 2;
                continue;
            }
        }
        i += 1;
    }
    out
}

fn extract_scopes(op: &Operation) -> Vec<String> {
    let mut set: BTreeSet<String> = BTreeSet::new();
    for sec in &op.security {
        for scopes in sec.values() {
            for s in scopes {
                set.insert(s.clone());
            }
        }
    }
    set.into_iter().collect()
}

fn validate_tool_name_lengths(tools: &[GeneratedTool]) {
    let violations: Vec<_> = tools
        .iter()
        .filter(|t| t.name.len() >= MAX_TOOL_NAME_LENGTH)
        .collect();

    assert!(
        violations.is_empty(),
        "Tool names must be < {} characters (Claude Code silently rejects longer ones).\n\
         Violations:\n  {}",
        MAX_TOOL_NAME_LENGTH,
        violations
            .iter()
            .map(|t| format!("'{}' ({} chars)", t.name, t.name.len()))
            .collect::<Vec<_>>()
            .join("\n  ")
    );
}

fn validate_essentials_present(tools: &[GeneratedTool]) {
    let names: BTreeSet<&str> = tools.iter().map(|t| t.name.as_str()).collect();
    let missing: Vec<&&str> = ESSENTIALS.iter().filter(|e| !names.contains(*e)).collect();

    assert!(
        missing.is_empty(),
        "Essentials whitelist drift detected. {} operation(s) listed in `ESSENTIALS` \
         (build.rs) are no longer present in `openapi/accounting.json`:\n  - {}\n\n\
         Fix: either (a) update `ESSENTIALS` in build.rs to the renamed operationId, \
         or (b) revert openapi/accounting.json. Run `cargo run -p refresh-openapi -- --diff` \
         to see what changed upstream.",
        missing.len(),
        missing
            .iter()
            .map(|s| s.to_string())
            .collect::<Vec<_>>()
            .join("\n  - ")
    );
}

fn render_generated_code(tools: &[GeneratedTool], spec_version: &str) -> String {
    let mut out = String::new();
    out.push_str(
        "// AUTO-GENERATED — do not edit. Source: openapi/accounting.json + build.rs.\n\n",
    );
    out.push_str("pub struct GeneratedTool {\n");
    out.push_str("    pub name: &'static str,\n");
    out.push_str("    pub description: &'static str,\n");
    out.push_str("    pub method: &'static str,\n");
    out.push_str("    pub path_template: &'static str,\n");
    out.push_str("    pub input_schema_json: &'static str,\n");
    out.push_str("    pub scopes: &'static [&'static str],\n");
    out.push_str("    pub multipart: bool,\n");
    out.push_str("    pub binary_fields: &'static [&'static str],\n");
    out.push_str("}\n\n");

    out.push_str(&format!(
        "pub const SPEC_VERSION: &str = {:?};\n\n",
        spec_version
    ));

    out.push_str("pub const ESSENTIALS: &[&str] = &[\n");
    for name in ESSENTIALS {
        out.push_str(&format!("    {:?},\n", name));
    }
    out.push_str("];\n\n");

    out.push_str("pub const GENERATED_TOOLS: &[GeneratedTool] = &[\n");
    for t in tools {
        out.push_str("    GeneratedTool {\n");
        out.push_str(&format!("        name: {:?},\n", t.name));
        out.push_str(&format!("        description: {:?},\n", t.description));
        out.push_str(&format!("        method: {:?},\n", t.method));
        out.push_str(&format!("        path_template: {:?},\n", t.path_template));
        out.push_str(&format!(
            "        input_schema_json: {:?},\n",
            t.input_schema_json
        ));
        out.push_str("        scopes: &[");
        for (i, s) in t.scopes.iter().enumerate() {
            if i > 0 {
                out.push_str(", ");
            }
            out.push_str(&format!("{:?}", s));
        }
        out.push_str("],\n");
        out.push_str(if t.multipart {
            "        multipart: true,\n"
        } else {
            "        multipart: false,\n"
        });
        let binary_fields: Vec<String> =
            t.binary_fields.iter().map(|f| format!("{:?}", f)).collect();
        out.push_str("        binary_fields: &[");
        out.push_str(&binary_fields.join(", "));
        out.push_str("],\n");
        out.push_str("    },\n");
    }
    out.push_str("];\n");
    out
}
