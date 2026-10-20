//! One real upload with a 256-character `filename`.
//! Pennylane must answer 4xx naming `filename`, creating nothing.
//! Run only with `--ignored` and a `file_attachments:all` token.

#![cfg(feature = "integration-tests")]

mod common;

use bytes::Bytes;

use mcp_pennylane::client::{FormPart, MultipartBody, PennylaneClient};
use mcp_pennylane::error::ClientError;

const BASE_URL: &str = "https://app.pennylane.com";
const FILENAME_CHARS: usize = 256;

fn token() -> String {
    std::env::var("PENNYLANE_API_KEY")
        .expect("PENNYLANE_API_KEY must be set for integration tests (no silent skip)")
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
#[ignore = "sends one real upload to Pennylane; run it explicitly with --ignored"]
async fn live_filename_past_255_characters_is_rejected_naming_the_field() {
    let client = PennylaneClient::new(&token(), BASE_URL, false).expect("client builds");
    let filename = format!("{}.pdf", "a".repeat(FILENAME_CHARS - ".pdf".len()));
    assert_eq!(filename.chars().count(), FILENAME_CHARS);

    let body = MultipartBody {
        parts: vec![FormPart::Text {
            name: "filename".to_string(),
            value: filename.clone(),
        }],
        file_field: "file".to_string(),
        file_name: filename,
        content_type: "application/pdf",
        bytes: Bytes::from(common::minimal_pdf()),
    };

    let outcome = client
        .call_multipart("POST", "/api/external/v2/file_attachments", &[], &body)
        .await;

    match outcome {
        Err(ClientError::Http {
            status,
            body: upstream,
            ..
        }) => {
            assert!(
                (400..500).contains(&status),
                "expected a 4xx, got {status}: {upstream}"
            );
            assert!(
                upstream.to_string().contains("filename"),
                "the 4xx must name `filename`, got {status}: {upstream}"
            );
        }
        Ok(created) => panic!("Pennylane accepted a 256-character filename and created: {created}"),
        Err(other) => panic!("expected an HTTP 4xx from Pennylane, got {other:?}"),
    }
}
