use axum::{body::Bytes, http::StatusCode};
use std::time::Duration;
use exavault::pkg::idempotency::{IdempotencyEntry, IdempotencyStore};

#[tokio::test]
async fn test_idempotency_store_lifecycle() {
    let store = IdempotencyStore::new(Duration::from_secs(60), Duration::from_secs(5));
    let key = "test-idem-key-1";

    // 1. Initial lock succeeds
    assert!(store.try_lock(key).await);

    // 2. Second lock with same key while in-flight fails
    assert!(!store.try_lock(key).await);

    // 3. Entry is in progress
    let entry = store.get(key).await.expect("Entry should exist");
    match entry {
        IdempotencyEntry::InProgress { .. } => {}
        _ => panic!("Expected InProgress"),
    }

    // 4. Save completed response
    let body = Bytes::from_static(b"{\"success\":true}");
    store
        .save(
            key,
            StatusCode::OK,
            vec![("content-type".to_string(), "application/json".to_string())],
            body.clone(),
        )
        .await;

    // 5. Subsequent get returns Completed entry
    let completed = store.get(key).await.expect("Completed entry should exist");
    match completed {
        IdempotencyEntry::Completed {
            status, body: b, ..
        } => {
            assert_eq!(status, StatusCode::OK);
            assert_eq!(b, body);
        }
        _ => panic!("Expected Completed"),
    }
}
