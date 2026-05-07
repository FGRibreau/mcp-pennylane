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
