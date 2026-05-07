use serde_json::{json, Map, Value};
use std::collections::HashSet;

use crate::error::ClientError;
use crate::generated::{GeneratedTool, ESSENTIALS, GENERATED_TOOLS};

pub const META_SEARCH_TOOL: &str = "pennylane_search_tools";
pub const META_EXECUTE_TOOL: &str = "pennylane_execute";

pub fn lookup_tool(name: &str) -> Option<&'static GeneratedTool> {
    GENERATED_TOOLS.iter().find(|t| t.name == name)
}

pub fn essentials_set() -> HashSet<&'static str> {
    ESSENTIALS.iter().copied().collect()
}

/// Visible tool list, filtered by `readonly`.
pub fn visible_tools(readonly: bool) -> Vec<&'static GeneratedTool> {
    let essentials = essentials_set();
    GENERATED_TOOLS
        .iter()
        .filter(|t| essentials.contains(t.name))
        .filter(|t| !readonly || t.method == "GET")
        .collect()
}

pub fn search_tool_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "query": {
                "type": "string",
                "description": "Keyword(s) matched against operationId and summary. Try 'webhook', 'mandate', 'attachment', 'changelog'."
            },
            "limit": {
                "type": "integer",
                "description": "Max candidates to return (default 10).",
                "default": 10,
                "minimum": 1,
                "maximum": 50
            }
        },
        "required": ["query"]
    })
}

pub fn execute_tool_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "tool_name": {
                "type": "string",
                "description": "Pennylane operationId, e.g. `getWebhookSubscription` or `postCommercialDocumentAppendices`. Discover with pennylane_search_tools."
            },
            "params": {
                "type": "object",
                "description": "Flat object containing path params, query params, and body fields. The dispatcher splits them based on the OpenAPI path template.",
                "additionalProperties": true
            }
        },
        "required": ["tool_name"]
    })
}

/// Run the search meta-tool: fuzzy match on `name + description`.
pub fn search(query: &str, limit: usize, readonly: bool) -> Value {
    let q = query.to_lowercase();
    let matches: Vec<Value> = GENERATED_TOOLS
        .iter()
        .filter(|t| !readonly || t.method == "GET")
        .filter(|t| {
            t.name.to_lowercase().contains(&q) || t.description.to_lowercase().contains(&q)
        })
        .take(limit)
        .map(|t| {
            json!({
                "tool_name": t.name,
                "method": t.method,
                "path_template": t.path_template,
                "description": t.description,
                "scopes": t.scopes,
                "input_schema": serde_json::from_str::<Value>(t.input_schema_json).unwrap_or(Value::Null),
            })
        })
        .collect();

    json!({
        "query": query,
        "match_count": matches.len(),
        "matches": matches,
    })
}

/// `(interpolated_path, query_pairs, optional_body)`.
pub type SplitParams = (String, Vec<(String, String)>, Option<Value>);

/// Decompose a flat `params` object into path/query/body buckets.
///
/// The OpenAPI path template (e.g. `/api/external/v2/customers/{id}`) drives
/// path-param extraction; remaining keys become query params if they're declared
/// in the operation's `parameters`, otherwise body fields (for POST/PUT/PATCH).
pub fn split_params(
    tool: &GeneratedTool,
    params: &Map<String, Value>,
) -> Result<SplitParams, ClientError> {
    let placeholders = path_placeholders(tool.path_template);
    let mut interpolated = tool.path_template.to_string();
    let mut consumed: HashSet<String> = HashSet::new();

    for ph in &placeholders {
        let raw = params.get(ph).ok_or_else(|| ClientError::InvalidArgs {
            operation_id: tool.name.to_string(),
            message: format!("missing required path parameter `{}`", ph),
        })?;
        let value_string = match raw {
            Value::String(s) => s.clone(),
            Value::Number(n) => n.to_string(),
            Value::Bool(b) => b.to_string(),
            other => other.to_string(),
        };
        interpolated = interpolated.replace(&format!("{{{}}}", ph), &value_string);
        consumed.insert(ph.clone());
    }

    let schema: Value =
        serde_json::from_str(tool.input_schema_json).unwrap_or(Value::Object(Map::new()));
    let declared_props: HashSet<String> = schema
        .get("properties")
        .and_then(|p| p.as_object())
        .map(|m| m.keys().cloned().collect())
        .unwrap_or_default();

    let mut query: Vec<(String, String)> = Vec::new();
    let mut body = Map::new();

    let is_get = tool.method == "GET" || tool.method == "DELETE";

    for (k, v) in params.iter() {
        if consumed.contains(k) {
            continue;
        }

        if is_get || declared_props.contains(k) && param_is_query(tool, k) {
            let s = match v {
                Value::String(s) => s.clone(),
                Value::Number(n) => n.to_string(),
                Value::Bool(b) => b.to_string(),
                other => other.to_string(),
            };
            query.push((k.clone(), s));
            continue;
        }

        body.insert(k.clone(), v.clone());
    }

    let body_value = if body.is_empty() {
        None
    } else {
        Some(Value::Object(body))
    };

    Ok((interpolated, query, body_value))
}

fn param_is_query(_tool: &GeneratedTool, _key: &str) -> bool {
    // Without a richer parameter-type table in GeneratedTool we treat any
    // non-path field on a write operation as body. Refine if a real op needs
    // mixed query+body on write — none observed in the current spec.
    false
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn essentials_present_in_generated() {
        let names: HashSet<&str> = GENERATED_TOOLS.iter().map(|t| t.name).collect();
        for e in ESSENTIALS {
            assert!(names.contains(e), "essential `{}` missing from spec", e);
        }
    }

    #[test]
    fn essentials_count_under_limit() {
        // The Claude Code soft cap is ~80 visible tools. Keep room for the 2 meta-tools.
        assert!(
            ESSENTIALS.len() <= 78,
            "essentials count {} exceeds the 78 budget; trim before adding more",
            ESSENTIALS.len()
        );
    }

    #[test]
    fn readonly_filter_strips_writes() {
        let visible = visible_tools(true);
        for t in &visible {
            assert_eq!(
                t.method, "GET",
                "non-GET tool `{}` slipped past the readonly filter",
                t.name
            );
        }
    }

    #[test]
    fn split_params_interpolates_path() {
        let tool = lookup_tool("getCustomer").expect("getCustomer present");
        let mut params = Map::new();
        params.insert("id".into(), Value::String("42".into()));
        let (path, query, body) = split_params(tool, &params).expect("split");
        assert!(path.ends_with("/customers/42"));
        assert!(query.is_empty());
        assert!(body.is_none());
    }

    #[test]
    fn split_params_requires_path_param() {
        let tool = lookup_tool("getCustomer").expect("getCustomer present");
        let err = split_params(tool, &Map::new()).unwrap_err();
        match err {
            ClientError::InvalidArgs { .. } => {}
            other => panic!("expected InvalidArgs, got {:?}", other),
        }
    }

    #[test]
    fn split_params_get_collects_query() {
        let tool = lookup_tool("getCustomers").expect("getCustomers present");
        let mut params = Map::new();
        params.insert("limit".into(), Value::Number(10.into()));
        params.insert("cursor".into(), Value::String("abc".into()));
        let (_, query, body) = split_params(tool, &params).expect("split");
        assert!(body.is_none());
        let names: HashSet<&str> = query.iter().map(|(k, _)| k.as_str()).collect();
        assert!(names.contains("limit"));
        assert!(names.contains("cursor"));
    }

    #[test]
    fn split_params_post_collects_body() {
        let tool = lookup_tool("postSupplier").expect("postSupplier present");
        let mut params = Map::new();
        params.insert("name".into(), Value::String("ACME".into()));
        let (_, query, body) = split_params(tool, &params).expect("split");
        assert!(query.is_empty());
        let body_obj = body.expect("body").as_object().cloned().expect("object");
        assert_eq!(body_obj.get("name").and_then(|v| v.as_str()), Some("ACME"));
    }
}
