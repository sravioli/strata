//! L1 tests of the upload routes (specs document-upload and job-queue) against PostgreSQL and
//! Garage, through the router of `strata api`.

mod common;

use std::sync::Arc;
use std::time::Duration;

use axum::Router;
use axum::body::{Body, to_bytes};
use axum::http::{HeaderMap, Request, StatusCode};
use axum::routing::get;
use common::*;
use serde_json::{Value, json};
use strata::api::{AppState, router};
use strata::auth::{Auth, keys::KeyCache};
use strata::db::{self, Pool, testing::tenant_tx, upload as q};
use strata::upload::Bounds;
use strata_core::revision::{Event, revision_step};
use strata_dev::DevKey;
use tokio_postgres::Client;
use tower::ServiceExt;
use uuid::Uuid;

// ---- The world of a test: two tenants with one account each, and the router.

struct Tenant {
    id: Uuid,
    token: String,
}

struct World {
    su: Client,
    app: Router,
    /// The database of a world in a new database, or empty.
    db: String,
    a: Tenant,
    b: Tenant,
    _key: DevKey,
}

/// A JWKS server for `key`.
async fn jwks(key: &DevKey) -> String {
    let body = key.jwks();
    let app = Router::new().route(
        "/jwks.json",
        get(move || {
            let body = body.clone();
            async move { axum::Json(body) }
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}/jwks.json", listener.local_addr().unwrap());
    tokio::spawn(axum::serve(listener, app).into_future());
    url
}

fn app(pool: Pool, bounds: Bounds) -> Router {
    router(AppState {
        auth: Auth::new(pool, Arc::new(KeyCache::new())),
        storage: Arc::new(storage()),
        bounds,
        status_retry_after: strata::status::DEFAULT_RETRY_AFTER,
    })
}

/// Two tenants in the database of `su`, and the router on `pool`.
async fn world_in(su: Client, pool: Pool, bounds: Bounds) -> World {
    let key = DevKey::generate();
    let (issuer_id, name) = issuer(&su, &jwks(&key).await).await;
    let mut tenants = Vec::new();
    for org in ["acme-corp", "globex"] {
        let id = tenant(&su).await;
        bind(&su, issuer_id, org, id).await;
        let now = strata_dev::now();
        let claims = json!({"iss": name, "aud": "strata", "org": org, "sub": unique("u"),
                            "iat": now - 5, "exp": now + 300});
        tenants.push(Tenant {
            id,
            token: key.sign(&claims),
        });
    }
    let b = tenants.pop().unwrap();
    let a = tenants.pop().unwrap();
    World {
        su,
        db: String::new(),
        app: app(pool, bounds),
        a,
        b,
        _key: key,
    }
}

async fn world() -> World {
    world_in(connect(&superuser()).await, api_pool(), Bounds::default()).await
}

/// A world in a new database, for a test that counts the rows of a global table.
async fn world_in_new_database(bounds: Bounds) -> World {
    let db = new_database(&connect(&superuser()).await).await;
    let pool = db::pool(config_in("strata_api", &db), 4, db::POOL_WAIT).unwrap();
    let mut w = world_in(connect(&config_in("strata", &db)).await, pool, bounds).await;
    w.db = db;
    w
}

struct Answer {
    status: StatusCode,
    headers: HeaderMap,
    body: Value,
}

impl World {
    async fn call(&self, req: Request<Body>) -> Answer {
        let resp = self.app.clone().oneshot(req).await.unwrap();
        let (parts, body) = resp.into_parts();
        let bytes = to_bytes(body, 1 << 20).await.unwrap();
        Answer {
            status: parts.status,
            headers: parts.headers,
            body: serde_json::from_slice(&bytes).unwrap_or(Value::Null),
        }
    }

    async fn post(
        &self,
        t: &Tenant,
        path: &str,
        content_type: Option<&str>,
        body: Vec<u8>,
    ) -> Answer {
        let mut req = Request::post(path).header("authorization", format!("Bearer {}", t.token));
        if let Some(ct) = content_type {
            req = req.header("content-type", ct);
        }
        self.call(req.body(Body::from(body)).unwrap()).await
    }

    async fn start(&self, t: &Tenant, document: Option<Uuid>, body: Value) -> Answer {
        let path = match document {
            Some(d) => format!("/v1/documents/{d}/revisions"),
            None => "/v1/documents".to_owned(),
        };
        self.post(
            t,
            &path,
            Some("application/json"),
            body.to_string().into_bytes(),
        )
        .await
    }

    async fn finish(&self, t: &Tenant, document: Uuid, revision: i64) -> Answer {
        self.post(
            t,
            &format!("/v1/documents/{document}/revisions/{revision}/finish"),
            None,
            vec![],
        )
        .await
    }

    /// A start that the API accepts: the document and the revision.
    async fn started(
        &self,
        t: &Tenant,
        document: Option<Uuid>,
        size: usize,
    ) -> (Uuid, i64, url::Url) {
        let a = self.start(t, document, body(size)).await;
        assert_eq!(a.status, StatusCode::CREATED, "{}", a.body);
        (
            a.body["document_id"].as_str().unwrap().parse().unwrap(),
            a.body["revision"].as_i64().unwrap(),
            a.body["upload_url"].as_str().unwrap().parse().unwrap(),
        )
    }

    /// A revision in `AwaitingUpload` with an object of `size` bytes.
    async fn uploaded(&self, t: &Tenant, size: usize) -> (Uuid, i64) {
        let (d, r, url) = self.started(t, None, size).await;
        assert_eq!(put(&url, &vec![0u8; size], size, Duration::ZERO).await, 200);
        (d, r)
    }

    async fn state(&self, d: Uuid, r: i64) -> (String, Option<String>) {
        let row = self
            .su
            .query_one(
                "select state, error_class from revisions where document_id = $1 and revision = $2",
                &[&d, &(r as i32)],
            )
            .await
            .unwrap();
        (row.get(0), row.get(1))
    }

    async fn jobs(&self, d: Uuid, r: i64) -> i64 {
        self.su
            .query_one(
                "select count(*) from jobs where document_id = $1 and revision = $2",
                &[&d, &(r as i32)],
            )
            .await
            .unwrap()
            .get(0)
    }

    async fn slot_exists(&self, d: Uuid, r: i64) -> bool {
        self.su
            .query_one(
                "select exists (select from upload_slots s join revisions v using (slot_id) \
                 where v.document_id = $1 and v.revision = $2)",
                &[&d, &(r as i32)],
            )
            .await
            .unwrap()
            .get(0)
    }

    /// The rows of a tenant in `documents` and `revisions`.
    async fn rows(&self, t: &Tenant) -> (i64, i64) {
        let row = self
            .su
            .query_one(
                "select (select count(*) from documents where tenant_id = $1), \
                 (select count(*) from revisions where tenant_id = $1)",
                &[&t.id],
            )
            .await
            .unwrap();
        (row.get(0), row.get(1))
    }
}

fn body(size: usize) -> Value {
    json!({"file_name": "report.pdf", "content_type": "application/pdf", "size": size})
}

fn location(d: Uuid, r: i64) -> String {
    format!("/v1/documents/{d}/revisions/{r}")
}

fn assert_accepted(a: &Answer, d: Uuid, r: i64) {
    assert_eq!(a.status, StatusCode::ACCEPTED, "{}", a.body);
    assert_eq!(a.headers["location"], location(d, r).as_str());
}

// ---- The start of an upload.

#[tokio::test]
async fn start_of_a_new_document() {
    let w = world().await;
    let a = w.start(&w.a, None, body(10)).await;
    assert_eq!(a.status, StatusCode::CREATED, "{}", a.body);
    let d: Uuid = a.body["document_id"].as_str().unwrap().parse().unwrap();
    assert_eq!(a.body["revision"], 1);
    let expires = a.body["expires_at"].as_str().unwrap();
    assert!(
        expires.ends_with('Z') && !expires.contains('.'),
        "{expires}"
    );
    assert_eq!(w.state(d, 1).await, ("AwaitingUpload".into(), None));
    assert!(w.slot_exists(d, 1).await);
    // The key of the URL.
    let url: url::Url = a.body["upload_url"].as_str().unwrap().parse().unwrap();
    assert_eq!(
        url.path(),
        format!(
            "/strata/tenants/{}/documents/{d}/revisions/1/original",
            w.a.id
        )
    );
}

#[tokio::test]
async fn start_of_the_next_revision() {
    let w = world().await;
    let (d, _, _) = w.started(&w.a, None, 10).await;
    let (d2, r, _) = w.started(&w.a, Some(d), 10).await;
    assert_eq!((d2, r), (d, 2));
}

/// Cross-tenant: `POST /v1/documents/{document_id}/revisions` with a document of B.
#[tokio::test]
async fn a_start_for_a_document_of_a_different_tenant() {
    let w = world().await;
    let (d_b, _, _) = w.started(&w.b, None, 10).await;
    let other = w.start(&w.a, Some(d_b), body(10)).await;
    let missing = w
        .start(&w.a, Some(Uuid::from_u128(unique_number())), body(10))
        .await;
    assert_eq!(
        (other.status, missing.status),
        (StatusCode::NOT_FOUND, StatusCode::NOT_FOUND)
    );
    assert_eq!(other.body, missing.body);
    assert_eq!(
        w.rows(&w.b).await,
        (1, 1),
        "the document of B gets no new revision"
    );
    assert_eq!(w.rows(&w.a).await, (0, 0));
}

/// Cross-tenant: `POST /v1/documents` creates a document that B cannot reach.
#[tokio::test]
async fn a_new_document_of_a_cannot_be_reached_by_b() {
    let w = world().await;
    let (d, _, _) = w.started(&w.a, None, 10).await;
    assert_eq!(
        w.start(&w.b, Some(d), body(10)).await.status,
        StatusCode::NOT_FOUND
    );
    assert_eq!(w.finish(&w.b, d, 1).await.status, StatusCode::NOT_FOUND);
    assert_eq!(w.rows(&w.a).await, (1, 1));
}

#[tokio::test]
async fn the_start_body() {
    let w = world().await;
    let a = w.start(&w.a, None, body(104_857_601)).await;
    assert_eq!(a.status, StatusCode::PAYLOAD_TOO_LARGE);
    assert_eq!(
        w.start(&w.a, None, body(0)).await.status,
        StatusCode::CREATED
    );
    for bad in [
        json!({"content_type": "a", "size": 1}),
        json!({"file_name": "a", "content_type": "", "size": 1}),
        json!({"file_name": "x".repeat(1025), "content_type": "a", "size": 1}),
        json!({"file_name": "a", "content_type": "a", "size": -1}),
    ] {
        assert_eq!(
            w.start(&w.a, None, bad.clone()).await.status,
            StatusCode::BAD_REQUEST,
            "{bad}"
        );
    }
    let ok = body(1).to_string().into_bytes();
    let mut big = ok.clone();
    big.resize(16_385, b' ');
    let rules = [
        w.post(&w.a, "/v1/documents", None, ok.clone()).await.status,
        w.post(&w.a, "/v1/documents", Some("text/plain"), ok)
            .await
            .status,
        w.post(&w.a, "/v1/documents", Some("application/json"), big)
            .await
            .status,
    ];
    assert_eq!(
        rules,
        [
            StatusCode::BAD_REQUEST,
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            StatusCode::PAYLOAD_TOO_LARGE
        ]
    );
    // Only the start of size 0 made rows.
    assert_eq!(w.rows(&w.a).await, (1, 1));
}

/// TC-49-04
#[tokio::test]
async fn two_starts_at_the_same_time_get_two_numbers() {
    let w = world().await;
    let (d, _, _) = w.started(&w.a, None, 10).await;
    let (x, y) = tokio::join!(
        w.start(&w.a, Some(d), body(10)),
        w.start(&w.a, Some(d), body(10))
    );
    assert_eq!(
        (x.status, y.status),
        (StatusCode::CREATED, StatusCode::CREATED)
    );
    let mut got = [
        x.body["revision"].as_i64().unwrap(),
        y.body["revision"].as_i64().unwrap(),
    ];
    got.sort();
    assert_eq!(got, [2, 3]);
}

// ---- The bounds of a start. They count the rows of global tables, so each runs in a new database.

fn small() -> Bounds {
    Bounds {
        waiting_jobs: 2,
        upload_slots: 3,
    }
}

async fn assert_bound(w: &World) {
    let a = w.start(&w.a, None, body(10)).await;
    assert_eq!(a.status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(a.headers["retry-after"], "60");
    assert_eq!(w.rows(&w.a).await, (0, 0), "no document or revision");
}

/// TC-5-06
#[tokio::test]
async fn a_full_queue_rejects_the_start() {
    let w = world_in_new_database(small()).await;
    w.su.execute(
        "insert into jobs (tenant_id, kind, document_id, revision) \
         select $1, 'ProcessDocument', uuidv7(), 1 from generate_series(1, 2)",
        &[&w.b.id],
    )
    .await
    .unwrap();
    assert_bound(&w).await;
    let jobs: i64 =
        w.su.query_one("select count(*) from jobs", &[])
            .await
            .unwrap()
            .get(0);
    assert_eq!(jobs, 2, "no job for the upload");
}

#[tokio::test]
async fn too_many_unfinished_uploads_reject_the_start() {
    let w = world_in_new_database(small()).await;
    w.su.execute(
        "insert into upload_slots (created_at) select now() from generate_series(1, 3)",
        &[],
    )
    .await
    .unwrap();
    assert_bound(&w).await;
}

#[tokio::test]
async fn old_rows_do_not_count() {
    let w = world_in_new_database(small()).await;
    w.su.execute(
        "insert into upload_slots (created_at) select now() - interval '2 hours 1 minute' \
         from generate_series(1, 3)",
        &[],
    )
    .await
    .unwrap();
    assert_eq!(
        w.start(&w.a, None, body(10)).await.status,
        StatusCode::CREATED
    );
}

// ---- The finish of an upload.

/// TC-5-01, and "The finish writes the result of the function".
#[tokio::test]
async fn a_finish_with_the_object() {
    let w = world().await;
    let (d, r) = w.uploaded(&w.a, 100).await;
    assert_accepted(&w.finish(&w.a, d, r).await, d, r);
    assert_eq!(w.state(d, r).await, ("Uploaded".into(), None));
    assert!(!w.slot_exists(d, r).await);
    let job = w
        .su
        .query_one(
            "select tenant_id, kind, state, attempts, run_after <= now() and run_after > now() - interval '1 minute' \
             from jobs where document_id = $1 and revision = $2",
            &[&d, &(r as i32)],
        )
        .await
        .unwrap();
    assert_eq!(job.get::<_, Uuid>(0), w.a.id);
    assert_eq!(
        (
            job.get::<_, String>(1),
            job.get::<_, String>(2),
            job.get::<_, i32>(3),
            job.get::<_, bool>(4)
        ),
        ("ProcessDocument".into(), "Waiting".into(), 0, true)
    );
}

/// TC-5-01: when the transaction of the finish rolls back, no job exists.
#[tokio::test]
async fn a_finish_that_rolls_back_leaves_no_job() {
    let w = world().await;
    let (d, r) = w.uploaded(&w.a, 100).await;
    let mut tx = tenant_tx(&api_pool(), w.a.id).await.unwrap();
    let locked = q::lock_revision(&mut tx, d, r as i32)
        .await
        .unwrap()
        .unwrap();
    let next = revision_step(locked.revision, Event::Finish);
    q::write_finish(tx, d, r as i32, next, locked.slot_id, false)
        .await
        .unwrap();
    assert_eq!(w.jobs(d, r).await, 0);
    assert_eq!(w.state(d, r).await, ("AwaitingUpload".into(), None));
}

/// TC-6-03
#[tokio::test]
async fn a_file_above_the_maximum() {
    let w = world().await;
    let (d, r, url) = w.started(&w.a, None, 10).await;
    // The signed URL cannot limit the size: the client sends more than it declared.
    let big = vec![0u8; 104_857_601];
    assert_eq!(put(&url, &big, big.len(), Duration::ZERO).await, 200);
    assert_eq!(
        w.finish(&w.a, d, r).await.status,
        StatusCode::PAYLOAD_TOO_LARGE
    );
    assert_eq!(
        w.state(d, r).await,
        ("Failed".into(), Some("FileTooLarge".into()))
    );
    assert_eq!(w.jobs(d, r).await, 0);
    assert!(!w.slot_exists(d, r).await);
    let key = strata::storage::key(w.a.id, d, r as i32);
    assert_eq!(
        storage().size(&key).await.unwrap(),
        None,
        "the object is removed"
    );
    assert_eq!(
        w.finish(&w.a, d, r).await.status,
        StatusCode::PAYLOAD_TOO_LARGE
    );
    assert_eq!(w.jobs(d, r).await, 0);
}

/// TC-6-04
#[tokio::test]
async fn a_finish_with_no_object() {
    let w = world().await;
    let (d, r, _) = w.started(&w.a, None, 10).await;
    assert_eq!(w.finish(&w.a, d, r).await.status, StatusCode::CONFLICT);
    assert_eq!(w.state(d, r).await, ("AwaitingUpload".into(), None));
    assert_eq!(w.jobs(d, r).await, 0);
    assert!(w.slot_exists(d, r).await);
}

/// TC-6-05 and TC-47-09
#[tokio::test]
async fn a_second_finish_changes_nothing() {
    let w = world().await;
    let (d, r) = w.uploaded(&w.a, 100).await;
    assert_accepted(&w.finish(&w.a, d, r).await, d, r);
    assert_accepted(&w.finish(&w.a, d, r).await, d, r);
    assert_eq!(w.jobs(d, r).await, 1);
}

/// TC-49-09
#[tokio::test]
async fn two_finishes_at_the_same_time_create_one_job() {
    let w = world().await;
    let (d, r) = w.uploaded(&w.a, 100).await;
    let (x, y) = tokio::join!(w.finish(&w.a, d, r), w.finish(&w.a, d, r));
    let z = w.finish(&w.a, d, r).await;
    for a in [&x, &y, &z] {
        assert_accepted(a, d, r);
    }
    assert_eq!(w.state(d, r).await, ("Uploaded".into(), None));
    assert_eq!(w.jobs(d, r).await, 1);
}

/// Cross-tenant: `POST /v1/documents/{document_id}/revisions/{revision}/finish` with a revision of B.
#[tokio::test]
async fn a_finish_of_a_revision_of_a_different_tenant() {
    let w = world().await;
    let (d, r) = w.uploaded(&w.b, 100).await;
    let other = w.finish(&w.a, d, r).await;
    let missing = w.finish(&w.a, Uuid::from_u128(unique_number()), 1).await;
    assert_eq!(
        (other.status, missing.status),
        (StatusCode::NOT_FOUND, StatusCode::NOT_FOUND)
    );
    assert_eq!(other.body, missing.body);
    assert_eq!(w.state(d, r).await, ("AwaitingUpload".into(), None));
    assert_eq!(w.jobs(d, r).await, 0);
}

#[tokio::test]
async fn a_finish_of_a_revision_in_a_later_state() {
    let w = world().await;
    for (state, class) in [
        ("Uploaded", None),
        ("Ready", None),
        ("Superseded", None),
        ("Failed", Some("MalformedFile")),
    ] {
        let (d, _) = document(&w.su, w.a.id, "AwaitingUpload").await;
        w.su.execute(
            "update revisions set state = $2, error_class = $3 where document_id = $1",
            &[&d, &state, &class],
        )
        .await
        .unwrap();
        assert_accepted(&w.finish(&w.a, d, 1).await, d, 1);
        assert_eq!(
            w.state(d, 1).await,
            (state.to_owned(), class.map(str::to_owned))
        );
        assert_eq!(w.jobs(d, 1).await, 0, "{state}");
    }
}

/// An identifier that no row has.
fn unique_number() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos()
}

// ---- From the upload to the end of the job (task 6.1).

/// Uploads a fixture with `content_type` through the routes, finishes it, and runs its job with
/// the real `strata-extract` and `pdftotext` in the sandbox. Gives the world and the document.
async fn extracted(fixture: &str, content_type: &str) -> (World, Uuid) {
    use strata::worker::{SETTINGS, Worker};
    // A new database: the worker claims any due job.
    let w = world_in_new_database(Bounds::default()).await;
    let bytes =
        std::fs::read(format!("{}/fixtures/{fixture}", env!("CARGO_MANIFEST_DIR"))).unwrap();
    let a = w
        .start(
            &w.a,
            None,
            json!({"file_name": "report.pdf", "content_type": content_type, "size": bytes.len()}),
        )
        .await;
    assert_eq!(a.status, StatusCode::CREATED, "{}", a.body);
    let d: Uuid = a.body["document_id"].as_str().unwrap().parse().unwrap();
    let url: url::Url = a.body["upload_url"].as_str().unwrap().parse().unwrap();
    assert_eq!(put(&url, &bytes, bytes.len(), Duration::ZERO).await, 200);
    assert_accepted(&w.finish(&w.a, d, 1).await, d, 1);
    let worker = Worker {
        pool: db::pool(config_in("strata_worker", &w.db), 4, db::POOL_WAIT).unwrap(),
        storage: storage(),
        sandbox: sandbox(&strata_extract(), None, strata::sandbox::LIMITS),
        settings: SETTINGS,
    };
    assert!(worker.run_one().await.unwrap().is_some(), "the job runs");
    (w, d)
}

impl World {
    /// The state of the job of revision 1, `no_text`, and the (page, text) of each text part.
    async fn result(&self, d: Uuid) -> (String, bool, Vec<(i32, String)>) {
        let job: String = self
            .su
            .query_one("select state from jobs where document_id = $1", &[&d])
            .await
            .unwrap()
            .get(0);
        let no_text: bool = self
            .su
            .query_one(
                "select no_text from revisions where document_id = $1 and revision = 1",
                &[&d],
            )
            .await
            .unwrap()
            .get(0);
        let parts = self
            .su
            .query(
                "select location_number, part_text from text_parts \
                 where document_id = $1 and revision = 1 order by part",
                &[&d],
            )
            .await
            .unwrap()
            .iter()
            .map(|r| (r.get(0), r.get(1)))
            .collect();
        (job, no_text, parts)
    }
}

fn two_pages() -> Vec<(i32, String)> {
    vec![(1, "alpha page one".into()), (2, "bravo page two".into())]
}

/// Scenario "The known text of a PDF".
#[tokio::test]
async fn the_known_text_of_a_pdf() {
    let (w, d) = extracted("two-pages.pdf", "application/pdf").await;
    assert_eq!(w.state(d, 1).await, ("Ready".into(), None));
    assert_eq!(w.result(d).await, ("Done".into(), false, two_pages()));
    let ready: Option<i32> =
        w.su.query_one(
            "select ready_revision from documents where document_id = $1",
            &[&d],
        )
        .await
        .unwrap()
        .get(0);
    assert_eq!(ready, Some(1));
}

/// Scenario "A PDF with a content type that is not PDF".
#[tokio::test]
async fn a_pdf_with_a_content_type_that_is_not_pdf() {
    let (w, d) = extracted("two-pages.pdf", "text/plain").await;
    assert_eq!(w.state(d, 1).await, ("Ready".into(), None));
    assert_eq!(w.result(d).await.2, two_pages());
}

/// TC-7-07, and the scenario "A format that is not PDF".
#[tokio::test]
async fn tc_7_07_a_zip_archive_with_the_name_report_pdf() {
    let (w, d) = extracted("report.pdf", "application/pdf").await;
    assert_eq!(
        w.state(d, 1).await,
        ("Failed".into(), Some("UnsupportedFormat".into()))
    );
    assert_eq!(w.result(d).await, ("Done".into(), false, vec![]));
}

/// TC-7-08
#[tokio::test]
async fn tc_7_08_a_scanned_pdf_with_no_text() {
    let (w, d) = extracted("scan-no-text.pdf", "application/pdf").await;
    assert_eq!(w.state(d, 1).await, ("Ready".into(), None), "not Failed");
    assert_eq!(w.result(d).await, ("Done".into(), true, vec![]), "NoText");
}

/// Scenario "A PDF with a user password".
#[tokio::test]
async fn a_pdf_with_a_user_password() {
    let (w, d) = extracted("user-password.pdf", "application/pdf").await;
    assert_eq!(
        w.state(d, 1).await,
        ("Failed".into(), Some("EncryptedFile".into()))
    );
    assert_eq!(w.result(d).await, ("Done".into(), false, vec![]));
}

/// Scenario "A PDF with only an owner password".
#[tokio::test]
async fn a_pdf_with_only_an_owner_password() {
    let (w, d) = extracted("owner-password.pdf", "application/pdf").await;
    assert_eq!(w.state(d, 1).await, ("Ready".into(), None));
    assert_eq!(w.result(d).await.2, two_pages());
}

/// Scenario "A PDF with a broken structure".
#[tokio::test]
async fn a_pdf_with_a_broken_structure() {
    let (w, d) = extracted("broken.pdf", "application/pdf").await;
    assert_eq!(
        w.state(d, 1).await,
        ("Failed".into(), Some("MalformedFile".into()))
    );
    assert_eq!(w.result(d).await, ("Done".into(), false, vec![]));
}
