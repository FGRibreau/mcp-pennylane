//! Read-only integration tests against the real Pennylane Company API v2.
//!
//! Enabled by `--features integration-tests`. Requires `PENNYLANE_API_KEY` to
//! be set to a valid Pennylane token. Tests fail-fast if the env var is
//! missing — never silently skip (per repo policy).
//!
//! Every test here is read-only (`GET` only). Writes are explicitly out of
//! scope to avoid polluting any real Pennylane account from CI.

#![cfg(feature = "integration-tests")]

use mcp_pennylane::client::{probe_readonly_from_me, PennylaneClient};
use mcp_pennylane::config::scopes_imply_readonly;

const BASE_URL: &str = "https://app.pennylane.com";

fn token() -> String {
    std::env::var("PENNYLANE_API_KEY")
        .expect("PENNYLANE_API_KEY must be set for integration tests (no silent skip)")
}

fn build_client() -> PennylaneClient {
    PennylaneClient::new(&token(), BASE_URL, false).expect("client builds")
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn live_get_me_returns_user_and_company() {
    let client = build_client();
    let response = client
        .call("GET", "/api/external/v2/me", &[], None)
        .await
        .expect("getMe should return 200");

    assert!(
        response.get("user").is_some(),
        "live /me must include `user`"
    );
    assert!(
        response.get("company").is_some(),
        "live /me must include `company`"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn live_get_me_carries_scopes_array() {
    // The vendored OpenAPI spec doesn't document the `scopes` field, but the
    // real API returns it. This test pins the runtime contract so a future
    // upstream removal makes us notice.
    let client = build_client();
    let response = client
        .call("GET", "/api/external/v2/me", &[], None)
        .await
        .expect("getMe should return 200");

    let scopes = response
        .get("scopes")
        .and_then(|v| v.as_array())
        .expect("live /me must carry a `scopes` array");
    assert!(
        !scopes.is_empty(),
        "live /me `scopes` array must be non-empty for a token with permissions"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn live_get_customers_returns_cursor_pagination() {
    let client = build_client();
    let response = client
        .call(
            "GET",
            "/api/external/v2/customers",
            &[("limit".into(), "1".into())],
            None,
        )
        .await
        .expect("getCustomers should return 200");

    assert!(
        response.get("items").is_some(),
        "list response must include `items`"
    );
    assert!(
        response.get("has_more").is_some(),
        "list response must include `has_more`"
    );
    // `next_cursor` may be null when has_more=false; the key itself must exist.
    assert!(
        response
            .as_object()
            .is_some_and(|o| o.contains_key("next_cursor")),
        "list response must include `next_cursor` (even if null)"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn live_probe_readonly_matches_pure_classifier() {
    // End-to-end check: the probe and the pure helper must agree on the same
    // scope vector. If they ever diverge, the helper has drifted from the
    // probe's parsing.
    let client = build_client();
    let response = client
        .call("GET", "/api/external/v2/me", &[], None)
        .await
        .expect("getMe should return 200");

    let scopes: Vec<String> = response
        .get("scopes")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|s| s.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default();
    let pure = scopes_imply_readonly(&scopes);

    let probed = probe_readonly_from_me(&client)
        .await
        .expect("probe should succeed");

    assert_eq!(
        pure, probed,
        "probe disagreed with pure classifier on the same /me scopes"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn live_unknown_customer_returns_structured_404() {
    use mcp_pennylane::error::ClientError;

    let client = build_client();
    let result = client
        .call("GET", "/api/external/v2/customers/999999999999", &[], None)
        .await;

    let err = result.expect_err("unknown customer must return an error");
    match err {
        ClientError::Http { status, .. } => {
            assert_eq!(status, 404, "expected 404, got {}", status);
        }
        other => panic!("expected ClientError::Http(404), got {:?}", other),
    }
}
