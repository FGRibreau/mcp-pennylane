//! JSON operations keep the request body `split_params` builds.
//! Six fixed bodies cover POST, PUT, GET and nested calls.
//! Multipart support must not alter what these send.

use serde_json::{json, Map, Value};

use mcp_pennylane::tools::{lookup_tool, split_params};

fn params(value: Value) -> Map<String, Value> {
    match value {
        Value::Object(map) => map,
        other => panic!("test params must be an object, got {other}"),
    }
}

fn sorted(mut query: Vec<(String, String)>) -> Vec<(String, String)> {
    query.sort();
    query
}

#[test]
fn post_without_path_param_sends_every_field_as_json_body() {
    let tool = lookup_tool("postSupplier").expect("postSupplier present");
    let input = json!({"name": "ACME", "emails": ["compta@acme.test"], "postal_address": {"city": "Nantes"}});
    let (path, query, body) = split_params(tool, &params(input.clone())).expect("split");
    assert_eq!(path, "/api/external/v2/suppliers");
    assert!(query.is_empty());
    assert_eq!(body, Some(input));
}

#[test]
fn put_consumes_the_path_param_and_keeps_the_rest_as_body() {
    let tool = lookup_tool("putSupplier").expect("putSupplier present");
    let (path, query, body) = split_params(
        tool,
        &params(json!({"id": 42, "name": "ACME", "use_2026_api_changes": true})),
    )
    .expect("split");
    assert_eq!(path, "/api/external/v2/suppliers/42");
    assert!(query.is_empty());
    assert_eq!(
        body,
        Some(json!({"name": "ACME", "use_2026_api_changes": true}))
    );
}

#[test]
fn get_sends_every_field_as_query_and_no_body() {
    let tool = lookup_tool("getCustomers").expect("getCustomers present");
    let (path, query, body) = split_params(
        tool,
        &params(json!({"limit": 10, "cursor": "abc", "filter": "[{\"field\":\"name\"}]"})),
    )
    .expect("split");
    assert_eq!(path, "/api/external/v2/customers");
    assert_eq!(
        sorted(query),
        vec![
            ("cursor".to_string(), "abc".to_string()),
            ("filter".to_string(), "[{\"field\":\"name\"}]".to_string()),
            ("limit".to_string(), "10".to_string()),
        ]
    );
    assert_eq!(body, None);
}

#[test]
fn nested_json_body_is_forwarded_verbatim() {
    let tool = lookup_tool("postCustomerInvoices").expect("postCustomerInvoices present");
    let input = json!({
        "customer_id": 7,
        "date": "2026-10-01",
        "deadline": "2026-10-31",
        "invoice_lines": [
            {"label": "Audit", "quantity": 2, "raw_currency_unit_price": "450.00", "unit": "day", "vat_rate": "FR_200"}
        ]
    });
    let (path, query, body) = split_params(tool, &params(input.clone())).expect("split");
    assert_eq!(path, "/api/external/v2/customer_invoices");
    assert!(query.is_empty());
    assert_eq!(body, Some(input));
}

#[test]
fn json_operation_with_a_path_param_and_a_body() {
    let tool = lookup_tool("sendByEmailQuote").expect("sendByEmailQuote present");
    let (path, query, body) = split_params(
        tool,
        &params(json!({"id": "q-9", "recipients": ["recipient@example.test"]})),
    )
    .expect("split");
    assert_eq!(path, "/api/external/v2/quotes/q-9/send_by_email");
    assert!(query.is_empty());
    assert_eq!(
        body,
        Some(json!({"recipients": ["recipient@example.test"]}))
    );
}

#[test]
fn operation_without_body_or_query_yields_nothing_else() {
    let tool = lookup_tool("markAsPaidCustomerInvoice").expect("present");
    let (path, query, body) = split_params(tool, &params(json!({"id": 3}))).expect("split");
    assert_eq!(path, "/api/external/v2/customer_invoices/3/mark_as_paid");
    assert!(query.is_empty());
    assert_eq!(body, None);
}
