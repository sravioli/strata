//! L1 tests of the status routes (spec revision-status) through the router of `strata api`.

mod common;

use axum::http::StatusCode;
use common::api::*;
use common::*;
use strata::api::router;
use strata::upload::Bounds;
use tokio_postgres::Client;
use uuid::Uuid;

fn path(d: Uuid, r: i32) -> String {
    format!("/v1/documents/{d}/revisions/{r}")
}

/// A new document of the tenant with revision 1 in `state`. `Failed` gets `MalformedFile`.
async fn revision(su: &Client, tenant: Uuid, state: &str) -> Uuid {
    document(su, tenant, state).await.0
}

/// Revision 2 of the document in `state`.
async fn second(su: &Client, tenant: Uuid, doc: Uuid, state: &str) {
    su.execute(
        "with r as (insert into revisions (tenant_id, document_id, revision, state, file_name, \
         content_type, size, slot_id) \
         values ($1, $2, 2, $3, 'b.pdf', 'application/pdf', 10, gen_random_uuid())) \
         update documents set last_revision = 2 where document_id = $2",
        &[&tenant, &doc, &state],
    )
    .await
    .unwrap();
}

async fn set_state(su: &Client, doc: Uuid, r: i32, state: &str, class: Option<&str>) {
    su.execute(
        "update revisions set state = $3, error_class = $4 where document_id = $1 and revision = $2",
        &[&doc, &r, &state, &class],
    )
    .await
    .unwrap();
}

/// TC-47-01
#[tokio::test]
async fn tc_47_01_each_internal_state() {
    let w = world().await;
    for (state, public) in [
        ("AwaitingUpload", "uploading"),
        ("Uploaded", "processing"),
        ("Ready", "ready"),
        ("Failed", "failed"),
        ("Superseded", "superseded"),
    ] {
        let d = revision(&w.su, w.a.id, state).await;
        let a = w.get(&w.a, &path(d, 1)).await;
        assert_eq!(a.status, StatusCode::OK, "{state}");
        assert_eq!(a.body["state"], public, "{state}");
        assert_eq!(a.body["document_id"], d.to_string());
        assert_eq!(a.body["revision"], 1);
        assert_eq!(a.headers["cache-control"], "no-store");
    }
}

/// TC-47-02
#[tokio::test]
async fn tc_47_02_a_parked_job() {
    let w = world().await;
    let d = revision(&w.su, w.a.id, "Uploaded").await;
    w.su.execute(
        "insert into jobs (tenant_id, kind, document_id, revision, state, attempts, last_error) \
         values ($1, 'ProcessDocument', $2, 1, 'Parked', 5, 'JobFailed')",
        &[&w.a.id, &d],
    )
    .await
    .unwrap();
    assert_eq!(w.get(&w.a, &path(d, 1)).await.body["state"], "processing");
}

/// TC-47-03
#[tokio::test]
async fn tc_47_03_a_newer_revision_is_ready() {
    let w = world().await;
    // The rows that the end of the job of revision 2 writes (spec revision-lifecycle).
    let d = revision(&w.su, w.a.id, "Superseded").await;
    second(&w.su, w.a.id, d, "Ready").await;
    assert_eq!(w.get(&w.a, &path(d, 1)).await.body["state"], "superseded");
    assert_eq!(w.get(&w.a, &path(d, 2)).await.body["state"], "ready");
}

/// TC-47-13. The end of a job writes `Superseded` only for an older revision in `AwaitingUpload`,
/// `Uploaded` or `Ready` (spec revision-lifecycle), so a `Failed` revision 1 stays `Failed`.
#[tokio::test]
async fn tc_47_13_failed_stays_failed() {
    let w = world().await;
    let d = revision(&w.su, w.a.id, "Failed").await;
    second(&w.su, w.a.id, d, "Ready").await;
    let a = w.get(&w.a, &path(d, 1)).await;
    assert_eq!(a.body["state"], "failed");
    assert_eq!(a.body["error_class"], "MalformedFile");

    let d = revision(&w.su, w.a.id, "Superseded").await;
    second(&w.su, w.a.id, d, "Ready").await;
    assert_eq!(w.get(&w.a, &path(d, 1)).await.body["state"], "superseded");
}

/// TC-47-04
#[tokio::test]
async fn tc_47_04_retry_after_and_the_final_statuses() {
    let w = world().await;
    for state in ["AwaitingUpload", "Uploaded"] {
        let d = revision(&w.su, w.a.id, state).await;
        assert_eq!(w.get(&w.a, &path(d, 1)).await.headers["retry-after"], "2");
    }
    for state in ["Ready", "Failed", "Superseded"] {
        let d = revision(&w.su, w.a.id, state).await;
        assert!(
            w.get(&w.a, &path(d, 1))
                .await
                .headers
                .get("retry-after")
                .is_none()
        );
    }
}

/// Scenario "The configured value".
#[tokio::test]
async fn the_configured_value() {
    let mut w = world().await;
    let mut state = test_state(api_pool(), Bounds::default());
    state.status_retry_after = 5;
    w.app = router(state);
    let d = revision(&w.su, w.a.id, "Uploaded").await;
    assert_eq!(w.get(&w.a, &path(d, 1)).await.headers["retry-after"], "5");
}

/// TC-47-10
#[tokio::test]
async fn tc_47_10_the_class_of_a_failed_revision() {
    let w = world().await;
    let d = revision(&w.su, w.a.id, "Failed").await;
    for class in ["FileTooLarge", "ExtractionTimeout", "InvalidOutput"] {
        set_state(&w.su, d, 1, "Failed", Some(class)).await;
        let a = w.get(&w.a, &path(d, 1)).await;
        assert_eq!(a.body["error_class"], class);
        let mut keys: Vec<&str> = a
            .body
            .as_object()
            .unwrap()
            .keys()
            .map(|k| k.as_str())
            .collect();
        keys.sort_unstable();
        assert_eq!(
            keys,
            [
                "document_id",
                "error_class",
                "revision",
                "state",
                "updated_at"
            ]
        );
    }
}

/// Scenario "A ready revision with no text".
#[tokio::test]
async fn a_ready_revision_with_no_text() {
    let w = world().await;
    let d = revision(&w.su, w.a.id, "Ready").await;
    w.su.execute(
        "update revisions set no_text = true where document_id = $1",
        &[&d],
    )
    .await
    .unwrap();
    let a = w.get(&w.a, &path(d, 1)).await;
    assert_eq!(
        (a.body["state"].as_str(), a.body["no_text"].as_bool()),
        (Some("ready"), Some(true))
    );
    assert!(a.body.get("error_class").is_none());
    let updated = a.body["updated_at"].as_str().unwrap();
    assert!(
        updated.ends_with('Z') && !updated.contains('.'),
        "{updated}"
    );
}

/// Cross-tenant: `GET /v1/documents/{document_id}/revisions/{revision}`. TC-47-06 (the read of
/// one revision): a revision of B, one that does not exist, and an identifier that is not valid
/// give the same `404`.
#[tokio::test]
async fn tc_47_06_a_revision_of_tenant_b() {
    let w = world().await;
    let of_b = revision(&w.su, w.b.id, "Ready").await;
    let answers = [
        w.get(&w.a, &path(of_b, 1)).await,
        w.get(&w.a, &path(Uuid::from_u128(0x5eed_0047_0006), 1))
            .await,
        w.get(&w.a, "/v1/documents/not-a-uuid/revisions/x").await,
    ];
    for a in &answers {
        assert_eq!(a.status, StatusCode::NOT_FOUND);
        assert_eq!(a.headers, answers[0].headers);
        assert_eq!(a.body, answers[0].body);
    }
    // B reads its own revision.
    assert_eq!(w.get(&w.b, &path(of_b, 1)).await.status, StatusCode::OK);
}

// ---- The batch status.

fn entry(d: Uuid, r: i32) -> serde_json::Value {
    serde_json::json!({"document_id": d.to_string(), "revision": r})
}

fn batch(entries: Vec<serde_json::Value>) -> serde_json::Value {
    serde_json::json!({ "revisions": entries })
}

/// Cross-tenant: `QUERY /v1/revisions`. TC-47-06 (the batch): the list omits a revision of B.
#[tokio::test]
async fn tc_47_06_the_batch_omits_a_revision_of_tenant_b() {
    let w = world().await;
    let of_a = revision(&w.su, w.a.id, "Ready").await;
    let of_b = revision(&w.su, w.b.id, "Ready").await;
    let a = w
        .query(
            &w.a,
            "/v1/revisions",
            &batch(vec![entry(of_b, 1), entry(of_a, 1)]),
        )
        .await;
    assert_eq!(a.status, StatusCode::OK);
    let list = a.body["revisions"].as_array().unwrap();
    assert_eq!(list.len(), 1);
    assert_eq!(list[0]["document_id"], of_a.to_string());
    let only_b = w
        .query(&w.a, "/v1/revisions", &batch(vec![entry(of_b, 1)]))
        .await;
    assert_eq!(only_b.body["revisions"].as_array().unwrap().len(), 0);
}

/// TC-47-07
#[tokio::test]
async fn tc_47_07_a_batch_that_is_not_valid() {
    let w = world().await;
    let many: Vec<_> = (0..101).map(|n| entry(Uuid::from_u128(n), 1)).collect();
    assert_eq!(
        w.query(&w.a, "/v1/revisions", &batch(many)).await.status,
        StatusCode::BAD_REQUEST
    );
    let bad = serde_json::json!({"revisions": [{"document_id": "x", "revision": 1}]});
    assert_eq!(
        w.query(&w.a, "/v1/revisions", &bad).await.status,
        StatusCode::BAD_REQUEST
    );
}

/// TC-47-08
#[tokio::test]
async fn tc_47_08_a_batch_with_one_status_that_is_not_final() {
    let w = world().await;
    let open = revision(&w.su, w.a.id, "Uploaded").await;
    let done = revision(&w.su, w.a.id, "Ready").await;
    let a = w
        .query(
            &w.a,
            "/v1/revisions",
            &batch(vec![entry(open, 1), entry(done, 1)]),
        )
        .await;
    let list = a.body["revisions"].as_array().unwrap();
    assert_eq!(list.len(), 2);
    assert_eq!(
        (list[0]["state"].as_str(), list[1]["state"].as_str()),
        (Some("processing"), Some("ready"))
    );
    assert_eq!(a.headers["retry-after"], "2");
    assert_eq!(a.headers["cache-control"], "no-store");
    assert_eq!(a.headers["accept-query"], "application/json");
    let final_only = w
        .query(&w.a, "/v1/revisions", &batch(vec![entry(done, 1)]))
        .await;
    assert!(final_only.headers.get("retry-after").is_none());
}

/// TC-47-15
#[tokio::test]
async fn tc_47_15_the_body_rules_of_the_batch() {
    let w = world().await;
    let body = batch(vec![entry(Uuid::from_u128(1), 1)])
        .to_string()
        .into_bytes();
    let none = w
        .send(&w.a, "QUERY", "/v1/revisions", None, body.clone())
        .await;
    assert_eq!(none.status, StatusCode::BAD_REQUEST);
    let text = w
        .send(&w.a, "QUERY", "/v1/revisions", Some("text/plain"), body)
        .await;
    assert_eq!(text.status, StatusCode::UNSUPPORTED_MEDIA_TYPE);
    assert_eq!(text.headers["accept-query"], "application/json");
    let large = vec![b' '; 16 * 1024 + 1];
    let large = w
        .send(
            &w.a,
            "QUERY",
            "/v1/revisions",
            Some("application/json"),
            large,
        )
        .await;
    assert_eq!(large.status, StatusCode::PAYLOAD_TOO_LARGE);
}

/// Scenario "A repeated entry".
#[tokio::test]
async fn a_repeated_entry() {
    let w = world().await;
    let d = revision(&w.su, w.a.id, "Ready").await;
    let a = w
        .query(
            &w.a,
            "/v1/revisions",
            &batch(vec![entry(d, 1), entry(d, 1)]),
        )
        .await;
    assert_eq!(a.body["revisions"].as_array().unwrap().len(), 1);
}

/// Scenario "A GET to the batch path".
#[tokio::test]
async fn a_get_to_the_batch_path() {
    let w = world().await;
    let a = w.get(&w.a, "/v1/revisions").await;
    assert_eq!(a.status, StatusCode::METHOD_NOT_ALLOWED);
    assert_eq!(a.headers["allow"], "QUERY");
    assert_eq!(a.headers["accept-query"], "application/json");
}
