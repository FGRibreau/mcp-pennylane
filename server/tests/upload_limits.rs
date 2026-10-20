//! Time and concurrency bounds of an upload.
//! Real files and a real semaphore, no network.

mod common;

use std::sync::Arc;
use std::time::Duration;

use tokio::sync::{OwnedSemaphorePermit, Semaphore};

use mcp_pennylane::error::ClientError;
use mcp_pennylane::upload::{
    acquire_upload_permit, load_file, UploadError, UploadLimits, MAX_CONCURRENT_UPLOADS,
    MAX_UPLOAD_BYTES, PERMIT_WAIT_TIMEOUT, READ_TIMEOUT,
};

use common::TempDir;

const SHORT_WAIT: Duration = Duration::from_millis(50);

fn slots() -> Arc<Semaphore> {
    Arc::new(Semaphore::new(MAX_CONCURRENT_UPLOADS))
}

async fn take_every_slot(uploads: &Arc<Semaphore>) -> Vec<OwnedSemaphorePermit> {
    let mut held = Vec::new();
    for _ in 0..MAX_CONCURRENT_UPLOADS {
        held.push(
            acquire_upload_permit(uploads, SHORT_WAIT)
                .await
                .expect("a free slot"),
        );
    }
    held
}

#[test]
fn the_bounds_are_the_documented_ones() {
    assert_eq!(MAX_CONCURRENT_UPLOADS, 4);
    assert_eq!(READ_TIMEOUT, Duration::from_secs(30));
    assert_eq!(PERMIT_WAIT_TIMEOUT, Duration::from_secs(30));
    assert_eq!(UploadLimits::DEFAULT.max_bytes, MAX_UPLOAD_BYTES);
}

#[tokio::test]
async fn four_uploads_hold_a_slot_and_the_fifth_is_refused() {
    let uploads = slots();
    let _held = take_every_slot(&uploads).await;
    assert_eq!(uploads.available_permits(), 0);

    match acquire_upload_permit(&uploads, SHORT_WAIT).await {
        Err(ClientError::UploadsBusy { limit, waited }) => {
            assert_eq!(limit, MAX_CONCURRENT_UPLOADS);
            assert_eq!(waited, SHORT_WAIT);
        }
        other => panic!("expected UploadsBusy, got {other:?}"),
    }
}

#[tokio::test]
async fn a_dropped_permit_frees_its_slot() {
    let uploads = slots();
    let mut held = take_every_slot(&uploads).await;
    drop(held.pop());
    assert_eq!(uploads.available_permits(), 1);

    let again = acquire_upload_permit(&uploads, SHORT_WAIT).await;
    assert!(again.is_ok(), "{again:?}");
}

#[tokio::test]
async fn a_waiting_upload_gets_the_slot_when_one_is_released() {
    let uploads = slots();
    let mut held = take_every_slot(&uploads).await;
    let waiting = {
        let uploads = Arc::clone(&uploads);
        tokio::spawn(async move {
            acquire_upload_permit(&uploads, PERMIT_WAIT_TIMEOUT)
                .await
                .map(drop)
        })
    };

    drop(held.pop());
    let joined = tokio::time::timeout(Duration::from_secs(10), waiting)
        .await
        .expect("the waiting upload wakes up")
        .expect("the task joins");
    assert!(joined.is_ok(), "{joined:?}");
}

#[test]
fn the_read_timeout_message_names_the_delay_and_the_likely_causes() {
    let message = UploadError::ReadTimedOut {
        limit: READ_TIMEOUT,
    }
    .to_string();
    assert!(
        message.contains("could not be read within 30s"),
        "{message}"
    );
    assert!(message.contains("cloud drive"), "{message}");
    assert!(message.contains("network mount"), "{message}");
}

#[tokio::test]
async fn a_read_slower_than_the_deadline_is_refused_with_the_timeout_message() {
    let dir = TempDir::new("slow-read");
    let path = dir.path().join("lent.pdf");
    let file = std::fs::File::create(&path).expect("create");
    file.set_len(MAX_UPLOAD_BYTES).expect("sparse file");
    drop(file);
    let limits = UploadLimits {
        read_timeout: Duration::from_millis(1),
        ..UploadLimits::DEFAULT
    };

    let outcome = load_file(
        "postFileAttachments",
        &path.display().to_string(),
        &[],
        limits,
    )
    .await;
    match outcome {
        Err(ClientError::InvalidArgs { message, .. }) => {
            assert!(
                message.contains("could not be read within 1ms"),
                "{message}"
            )
        }
        other => panic!("expected the timeout refusal, got {other:?}"),
    }
}
