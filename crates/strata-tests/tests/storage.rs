//! L1 tests of the storage: Garage and the signed URLs of `rusty-s3` (spec document-upload,
//! "Garage accepts the signed URLs"). They run before each other L1 test that uses Garage.

mod common;

use std::time::Duration;

use common::*;
use strata::storage::PUT_EXPIRY;

const NOW: Duration = Duration::ZERO;

fn new_key() -> String {
    format!("test/{}", unique("obj"))
}

/// A PUT before the expiry writes the object, and the API reads its size and removes it.
#[tokio::test]
async fn a_signed_put_writes_its_key() {
    let s = storage();
    let key = new_key();
    let body = vec![7u8; 1000];
    assert_eq!(
        put(&s.sign_put(&key, PUT_EXPIRY).0, &body, body.len(), NOW).await,
        200
    );
    assert_eq!(s.size(&key).await.unwrap(), Some(1000));
    s.remove(&key).await.unwrap();
    assert_eq!(s.size(&key).await.unwrap(), None);
    // A removal of a key with no object is not an error.
    s.remove(&key).await.unwrap();
}

/// TC-6-01
#[tokio::test]
async fn a_url_for_one_key_cannot_write_a_different_key() {
    let s = storage();
    let (key, other) = (new_key(), new_key());
    let mut url = s.sign_put(&key, PUT_EXPIRY).0;
    url.set_path(&format!("/strata/{other}"));
    assert_eq!(put(&url, b"x", 1, NOW).await, 403);
    assert_eq!(s.size(&other).await.unwrap(), None);
}

/// TC-6-02
#[tokio::test]
async fn an_expired_url_cannot_write() {
    let s = storage();
    let key = new_key();
    let url = s.sign_put(&key, Duration::from_secs(1)).0;
    tokio::time::sleep(Duration::from_secs(2)).await;
    // Garage answers 400 "Date is too old" for an expired URL.
    let status = put(&url, b"x", 1, NOW).await;
    assert!(matches!(status, 400 | 403), "{status}");
    assert_eq!(s.size(&key).await.unwrap(), None);
}

/// A slow transfer that starts in time completes: the storage checks the expiry only when the
/// request arrives (0006).
#[tokio::test]
async fn a_slow_transfer_that_starts_in_time_completes() {
    let s = storage();
    let key = new_key();
    let url = s.sign_put(&key, Duration::from_secs(2)).0;
    let body = vec![1u8; 64 * 1024];
    assert_eq!(put(&url, &body, 1024, Duration::from_secs(4)).await, 200);
    assert_eq!(s.size(&key).await.unwrap(), Some(body.len() as u64));
    s.remove(&key).await.unwrap();
}
