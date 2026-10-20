//! Invariants the vendored OpenAPI spec MUST hold. Black-box tests against the
//! generated module — no mocks, no spec re-parse at test time. Failures here
//! flag drift between the vendored spec and our assumptions.

use std::collections::HashSet;

use mcp_pennylane::generated::{ESSENTIALS, GENERATED_TOOLS, SPEC_VERSION};

#[test]
fn spec_version_is_present() {
    assert!(!SPEC_VERSION.is_empty(), "SPEC_VERSION must be populated");
}

#[test]
fn at_least_120_operations_generated() {
    assert!(
        GENERATED_TOOLS.len() >= 120,
        "expected at least 120 generated tools (current spec has 163), got {}",
        GENERATED_TOOLS.len()
    );
}

#[test]
fn all_tool_names_under_64_chars() {
    let violations: Vec<String> = GENERATED_TOOLS
        .iter()
        .filter(|t| t.name.len() >= 64)
        .map(|t| format!("{} ({} chars)", t.name, t.name.len()))
        .collect();
    assert!(
        violations.is_empty(),
        "Tool names must be < 64 chars (Claude Code soft limit). Violations:\n  {}",
        violations.join("\n  ")
    );
}

#[test]
fn all_essentials_resolve() {
    let names: HashSet<&str> = GENERATED_TOOLS.iter().map(|t| t.name).collect();
    let missing: Vec<&&str> = ESSENTIALS.iter().filter(|e| !names.contains(*e)).collect();
    assert!(
        missing.is_empty(),
        "essentials missing from spec: {:?}",
        missing
    );
}

#[test]
fn methods_are_canonical() {
    let allowed: HashSet<&str> = ["GET", "POST", "PUT", "PATCH", "DELETE"]
        .into_iter()
        .collect();
    for t in GENERATED_TOOLS {
        assert!(
            allowed.contains(t.method),
            "tool `{}` has non-canonical method `{}`",
            t.name,
            t.method
        );
    }
}

#[test]
fn input_schemas_parse_as_json() {
    for t in GENERATED_TOOLS {
        let parsed: Result<serde_json::Value, _> = serde_json::from_str(t.input_schema_json);
        assert!(
            parsed.is_ok(),
            "tool `{}` has invalid input_schema_json: {}",
            t.name,
            t.input_schema_json
        );
    }
}

#[test]
fn essentials_count_within_budget() {
    assert!(
        ESSENTIALS.len() <= 78,
        "essentials count {} exceeds the 78-tool budget (Claude Code visible-tool soft cap is ~80, leaving room for 2 meta-tools)",
        ESSENTIALS.len()
    );
}

#[test]
fn paths_use_v2_prefix() {
    for t in GENERATED_TOOLS {
        assert!(
            t.path_template.starts_with("/api/external/v2/"),
            "tool `{}` has non-v2 path `{}`",
            t.name,
            t.path_template
        );
    }
}

const MULTIPART_OPERATIONS: &[&str] = &[
    "postFileAttachments",
    "postLedgerAttachments",
    "createSupplierInvoiceEInvoiceImport",
    "createCustomerInvoiceEInvoiceImport",
    "createEInvoiceImport",
    "postCustomerInvoiceAppendices",
    "postQuoteAppendices",
    "postCommercialDocumentAppendices",
];

fn schema_of(name: &str) -> serde_json::Value {
    let tool = GENERATED_TOOLS
        .iter()
        .find(|t| t.name == name)
        .unwrap_or_else(|| panic!("operation `{}` missing from the spec", name));
    serde_json::from_str(tool.input_schema_json).expect("schema parses")
}

#[test]
fn multipart_operations_are_the_eight_the_spec_declares() {
    let mut found: Vec<&str> = GENERATED_TOOLS
        .iter()
        .filter(|t| t.multipart)
        .map(|t| t.name)
        .collect();
    let mut expected = MULTIPART_OPERATIONS.to_vec();
    found.sort_unstable();
    expected.sort_unstable();
    assert_eq!(found, expected);
}

#[test]
fn multipart_operations_are_posts_with_exactly_one_binary_field() {
    for t in GENERATED_TOOLS.iter().filter(|t| t.multipart) {
        assert_eq!(t.method, "POST", "`{}` must be a POST", t.name);
        assert_eq!(t.binary_fields, ["file"], "`{}` binary fields", t.name);
    }
}

#[test]
fn json_operations_have_no_binary_field() {
    for t in GENERATED_TOOLS.iter().filter(|t| !t.multipart) {
        assert!(t.binary_fields.is_empty(), "`{}` is not multipart", t.name);
    }
}

#[test]
fn every_multipart_schema_exposes_file_as_a_required_local_path() {
    for name in MULTIPART_OPERATIONS {
        let schema = schema_of(name);
        let file = &schema["properties"]["file"];
        assert_eq!(file["type"], "string", "`{}` file type", name);
        assert!(file.get("format").is_none(), "`{}` still says binary", name);
        let description = file["description"].as_str().expect("file description");
        assert!(
            description.starts_with("Absolute path of a local file"),
            "`{}`: {}",
            name,
            description
        );
        let required = schema["required"].as_array().expect("required list");
        assert!(
            required.iter().any(|r| r == "file"),
            "`{}` requires file",
            name
        );
    }
}

#[test]
fn file_attachment_schemas_expose_file_and_a_bounded_filename() {
    for name in ["postFileAttachments", "postLedgerAttachments"] {
        let schema = schema_of(name);
        assert_eq!(schema["properties"]["filename"]["type"], "string");
        assert_eq!(schema["properties"]["filename"]["maxLength"], 255);
        assert!(!schema["required"]
            .as_array()
            .expect("required list")
            .iter()
            .any(|r| r == "filename"));
    }
}

#[test]
fn only_the_two_attachment_operations_declare_filename() {
    let declaring: Vec<&str> = MULTIPART_OPERATIONS
        .iter()
        .copied()
        .filter(|name| schema_of(name)["properties"].get("filename").is_some())
        .collect();
    assert_eq!(declaring, ["postFileAttachments", "postLedgerAttachments"]);
}

#[test]
fn factur_x_schemas_expose_invoice_options_as_an_object() {
    for name in [
        "createSupplierInvoiceEInvoiceImport",
        "createCustomerInvoiceEInvoiceImport",
    ] {
        let schema = schema_of(name);
        assert_eq!(schema["properties"]["invoice_options"]["type"], "object");
        assert!(!schema["required"]
            .as_array()
            .expect("required list")
            .iter()
            .any(|r| r == "invoice_options"));
    }
}

#[test]
fn e_invoice_import_requires_its_type() {
    let schema = schema_of("createEInvoiceImport");
    assert_eq!(
        schema["properties"]["type"]["enum"],
        serde_json::json!(["customer", "supplier"])
    );
    let required = schema["required"].as_array().expect("required list");
    assert!(required.iter().any(|r| r == "type"));
}

#[test]
fn appendix_operations_keep_their_path_parameter() {
    for (name, parameter) in [
        ("postCustomerInvoiceAppendices", "customer_invoice_id"),
        ("postQuoteAppendices", "quote_id"),
        ("postCommercialDocumentAppendices", "commercial_document_id"),
    ] {
        let schema = schema_of(name);
        assert!(schema["properties"].get(parameter).is_some(), "`{}`", name);
        let required = schema["required"].as_array().expect("required list");
        assert!(required.iter().any(|r| r == parameter), "`{}`", name);
    }
}

#[test]
fn supplier_e_invoice_import_is_a_direct_tool_unless_readonly() {
    use mcp_pennylane::tools::visible_tools;

    assert!(ESSENTIALS.contains(&"createSupplierInvoiceEInvoiceImport"));
    let writable: Vec<&str> = visible_tools(false).iter().map(|t| t.name).collect();
    assert!(writable.contains(&"postFileAttachments"));
    assert!(writable.contains(&"createSupplierInvoiceEInvoiceImport"));
    let readonly: Vec<&str> = visible_tools(true).iter().map(|t| t.name).collect();
    for name in MULTIPART_OPERATIONS {
        assert!(
            !readonly.contains(name),
            "`{}` visible in read-only mode",
            name
        );
    }
}
