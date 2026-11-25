//! L1 tests of the storage: Garage and the signed URLs of `rusty-s3` (spec document-upload,
//! "Garage accepts the signed URLs"). They run before each other L1 test that uses Garage.

mod common;

use std::time::Duration;

use common::*;
use strata::storage::{Object, PUT_EXPIRY};

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
    assert_eq!(s.head(&key).await.unwrap().map(|h| h.size), Some(1000));
    s.remove(&key).await.unwrap();
    assert_eq!(s.head(&key).await.unwrap().map(|h| h.size), None);
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
    assert_eq!(s.head(&other).await.unwrap().map(|h| h.size), None);
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
    assert_eq!(s.head(&key).await.unwrap().map(|h| h.size), None);
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
    assert_eq!(
        s.head(&key).await.unwrap().map(|h| h.size),
        Some(body.len() as u64)
    );
    s.remove(&key).await.unwrap();
}

/// Task 5.1: the worker reads an object, and at most `max` + 1 bytes of a larger one.
#[tokio::test]
async fn the_worker_reads_at_most_the_limit_and_one_byte() {
    let s = storage();
    let key = new_key();
    let body: Vec<u8> = (0..=255).cycle().take(100_000).collect();
    let (url, _) = s.sign_put(&key, std::time::Duration::from_secs(60));
    assert_eq!(
        put(&url, &body, body.len(), std::time::Duration::ZERO).await,
        200
    );
    assert_eq!(
        s.read(&key, 100_000, None).await.unwrap(),
        Object::Bytes(body.clone())
    );
    assert_eq!(
        s.read(&key, 10, None).await.unwrap(),
        Object::Bytes(body[..11].to_vec())
    );
    assert_eq!(s.read(&new_key(), 10, None).await.unwrap(), Object::Missing);

    // The read of the ETag of a HEAD gives that object, and only that object.
    let head = s.head(&key).await.unwrap().unwrap();
    assert_eq!(
        s.read(&key, 100_000, Some(&head.etag)).await.unwrap(),
        Object::Bytes(body.clone())
    );
    let (url, _) = s.sign_put(&key, std::time::Duration::from_secs(60));
    assert_eq!(put(&url, b"other", 5, std::time::Duration::ZERO).await, 200);
    assert_eq!(
        s.read(&key, 100_000, Some(&head.etag)).await.unwrap(),
        Object::Changed
    );
}
