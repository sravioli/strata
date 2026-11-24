//! L1 tests of `QUERY /v1/search` (spec document-search) through the router of `strata api`.
//! Each test runs in a new database: the text of a revision comes from the end of its job, and
//! a claim takes any due job.

mod common;

use axum::http::StatusCode;
use common::api::*;
use common::*;
use serde_json::{Value, json};
use strata::db::job::{ClaimTx, Finish, end_job};
use strata::db::{self, Pool};
use strata::extraction::Part;
use strata::upload::Bounds;
use strata_core::revision::Event;
use uuid::Uuid;

struct S {
    w: World,
    worker: Pool,
}

async fn s() -> S {
    let w = world_in_new_database(Bounds::default()).await;
    let worker = db::pool(config_in("strata_worker", &w.db), 4, db::POOL_WAIT).unwrap();
    S { w, worker }
}

fn page(n: u32, text: &str) -> Part {
    Part {
        location: n,
        text: text.into(),
    }
}

impl S {
    /// Revision `n` of `doc` (a new document when `doc` is `None`) in `Uploaded` with its job,
    /// then the end of the job with `parts`. No part means `NoText`.
    async fn ready(&self, tenant: Uuid, doc: Option<Uuid>, parts: Vec<Part>) -> (Uuid, i32) {
        let su = &self.w.su;
        let (doc, n) = match doc {
            None => (document(su, tenant, "Uploaded").await.0, 1),
            Some(doc) => {
                let n: i32 = su
                    .query_one(
                        "update documents set last_revision = last_revision + 1 \
                         where document_id = $1 returning last_revision",
                        &[&doc],
                    )
                    .await
                    .unwrap()
                    .get(0);
                su.execute(
                    "insert into revisions (tenant_id, document_id, revision, state, file_name, \
                     content_type, size, slot_id) \
                     values ($1, $2, $3, 'Uploaded', 'a.pdf', 'application/pdf', 10, gen_random_uuid())",
                    &[&tenant, &doc, &n],
                )
                .await
                .unwrap();
                (doc, n)
            }
        };
        su.execute(
            "insert into jobs (tenant_id, kind, document_id, revision, run_after) \
             values ($1, 'ProcessDocument', $2, $3, now() - interval '1 second')",
            &[&tenant, &doc, &n],
        )
        .await
        .unwrap();
        let claim = ClaimTx::claim(&self.worker)
            .await
            .unwrap()
            .expect("a due job");
        let claimed = claim.claimed().clone();
        claim
            .into_tenant()
            .await
            .unwrap()
            .unwrap()
            .commit()
            .await
            .unwrap();
        let tx = ClaimTx::resume(&self.worker, &claimed).await.unwrap();
        let no_text = parts.is_empty();
        end_job(
            tx,
            &claimed,
            Finish::Extraction(Event::Extracted(no_text), parts),
        )
        .await
        .unwrap();
        (doc, n)
    }

    async fn search(&self, t: &Tenant, body: Value) -> Answer {
        self.w.query(t, "/v1/search", &body).await
    }
}

fn results(a: &Answer) -> Vec<(String, i64)> {
    a.body["results"]
        .as_array()
        .unwrap_or_else(|| panic!("{} {}", a.status, a.body))
        .iter()
        .map(|r| {
            (
                r["document_id"].as_str().unwrap().to_owned(),
                r["revision"].as_i64().unwrap(),
            )
        })
        .collect()
}

/// Cross-tenant: `QUERY /v1/search`. TC-49-05
#[tokio::test]
async fn tc_49_05_two_tenants_with_the_same_word() {
    let s = s().await;
    let (of_a, _) = s.ready(s.w.a.id, None, vec![page(1, "zircon of a")]).await;
    s.ready(s.w.b.id, None, vec![page(1, "zircon of b")]).await;
    let a = s.search(&s.w.a, json!({"q": "zircon"})).await;
    assert_eq!(a.status, StatusCode::OK);
    assert_eq!(results(&a), [(of_a.to_string(), 1)]);
}

/// TC-4-01
#[tokio::test]
async fn tc_4_01_two_accounts_of_one_tenant() {
    let s = s().await;
    let anna = s.w.token("acme-corp");
    let bruno = s.w.token("acme-corp");
    // Anna uploads, and the worker ends the job of her revision.
    let (doc, _) = s.w.uploaded(&anna, 10).await;
    assert_accepted(&s.w.finish(&anna, doc, 1).await, doc, 1);
    let claim = ClaimTx::claim(&s.worker).await.unwrap().unwrap();
    let claimed = claim.claimed().clone();
    claim
        .into_tenant()
        .await
        .unwrap()
        .unwrap()
        .commit()
        .await
        .unwrap();
    let tx = ClaimTx::resume(&s.worker, &claimed).await.unwrap();
    let text = Finish::Extraction(Event::Extracted(false), vec![page(1, "feldspar")]);
    end_job(tx, &claimed, text).await.unwrap();
    // Bruno finds it and reads it.
    assert_eq!(
        results(&s.search(&bruno, json!({"q": "feldspar"})).await),
        [(doc.to_string(), 1)]
    );
    let status =
        s.w.get(&bruno, &format!("/v1/documents/{doc}/revisions/1"))
            .await;
    assert_eq!(status.body["state"], "ready");
}

/// Scenario "A body with the default limit".
#[tokio::test]
async fn a_body_with_the_default_limit() {
    let s = s().await;
    for _ in 0..25 {
        s.ready(s.w.a.id, None, vec![page(1, "quartz")]).await;
    }
    assert_eq!(
        results(&s.search(&s.w.a, json!({"q": "quartz"})).await).len(),
        20
    );
    let three = s.search(&s.w.a, json!({"q": "quartz", "limit": 3})).await;
    assert_eq!(results(&three).len(), 3);
}

/// TC-49-07, and the scenario "A body of another media type".
#[tokio::test]
async fn tc_49_07_a_body_that_is_not_valid() {
    let w = world().await;
    let send = |ct: Option<&'static str>, body: Vec<u8>| {
        let w = &w;
        async move { w.send(&w.a, "QUERY", "/v1/search", ct, body).await.status }
    };
    let json = Some("application/json");
    assert_eq!(
        send(json, br#"{"limit": 5}"#.to_vec()).await,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        send(json, br#"{"q": "a", "limit": 101}"#.to_vec()).await,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        send(json, vec![b' '; 16 * 1024 + 1]).await,
        StatusCode::PAYLOAD_TOO_LARGE
    );
    assert_eq!(
        send(Some("text/plain"), br#"{"q": "a"}"#.to_vec()).await,
        StatusCode::UNSUPPORTED_MEDIA_TYPE
    );
    assert_eq!(
        send(None, br#"{"q": "a"}"#.to_vec()).await,
        StatusCode::BAD_REQUEST
    );
}

/// TC-49-08
#[tokio::test]
async fn tc_49_08_a_get_and_a_post() {
    let w = world().await;
    for method in ["GET", "POST"] {
        let a = w
            .send(
                &w.a,
                method,
                "/v1/search",
                Some("application/json"),
                br#"{"q": "a"}"#.to_vec(),
            )
            .await;
        assert_eq!(a.status, StatusCode::METHOD_NOT_ALLOWED, "{method}");
        assert_eq!(a.headers["allow"], "QUERY");
    }
}

/// Scenario "The page of the best match".
#[tokio::test]
async fn the_page_of_the_best_match() {
    let s = s().await;
    s.ready(
        s.w.a.id,
        None,
        vec![
            page(1, "quartz and mica"),
            page(2, "quartz quartz and more quartz"),
        ],
    )
    .await;
    let a = s.search(&s.w.a, json!({"q": "quartz"})).await;
    assert_eq!(
        a.body["results"][0]["location"],
        json!({"kind": "page", "number": 2})
    );
    assert!(a.body["results"][0]["score"].as_f64().unwrap() > 0.0);
    assert_eq!(
        a.body["results"].as_array().unwrap().len(),
        1,
        "one result for each document"
    );
}

/// Scenario "A query that PostgreSQL cannot parse as tsquery syntax".
#[tokio::test]
async fn a_query_with_no_tsquery_syntax() {
    let s = s().await;
    s.ready(s.w.a.id, None, vec![page(1, "quartz")]).await;
    for q in ["\"quartz", "-", "quartz & | !", "or or"] {
        let a = s.search(&s.w.a, json!({ "q": q })).await;
        assert_eq!(a.status, StatusCode::OK, "{q}");
        assert!(a.body["results"].is_array(), "{q}");
    }
}

/// TC-49-02: after revision 2 becomes `Ready`, a search returns revision 2 only.
#[tokio::test]
async fn tc_49_02_revision_2_becomes_ready() {
    let s = s().await;
    let (doc, _) = s.ready(s.w.a.id, None, vec![page(1, "basalt one")]).await;
    s.ready(s.w.a.id, Some(doc), vec![page(1, "basalt two")])
        .await;
    assert_eq!(
        results(&s.search(&s.w.a, json!({"q": "basalt"})).await),
        [(doc.to_string(), 2)]
    );
    assert!(results(&s.search(&s.w.a, json!({"q": "one"})).await).is_empty());
}

/// TC-49-06: a search for a word in the last of the four parts of a page of 200 KB returns the
/// page.
#[tokio::test]
async fn tc_49_06_a_page_of_200_kb() {
    let s = s().await;
    let page_text = "word ".repeat(200 * 1024 / 5 - 2) + "obsidian";
    let output = json!({"format": "pdf", "pages": 1,
        "blocks": [{"kind": "page", "index": 1, "text": page_text}]});
    let parts = strata::extraction::check(output.to_string().as_bytes()).unwrap();
    assert_eq!(parts.len(), 4);
    assert!(parts[3].text.contains("obsidian") && !parts[2].text.contains("obsidian"));
    let (doc, _) = s.ready(s.w.a.id, None, parts).await;
    let a = s.search(&s.w.a, json!({"q": "obsidian"})).await;
    assert_eq!(results(&a), [(doc.to_string(), 1)]);
    assert_eq!(
        a.body["results"][0]["location"],
        json!({"kind": "page", "number": 1})
    );
}

/// TC-47-17
#[tokio::test]
async fn tc_47_17_a_newer_revision_with_no_text() {
    let s = s().await;
    let (doc, _) = s.ready(s.w.a.id, None, vec![page(1, "granite")]).await;
    s.ready(s.w.a.id, Some(doc), Vec::new()).await;
    let r2 =
        s.w.get(&s.w.a, &format!("/v1/documents/{doc}/revisions/2"))
            .await;
    assert_eq!(
        (r2.body["state"].as_str(), r2.body["no_text"].as_bool()),
        (Some("ready"), Some(true))
    );
    let r1 =
        s.w.get(&s.w.a, &format!("/v1/documents/{doc}/revisions/1"))
            .await;
    assert_eq!(r1.body["state"], "superseded");
    assert!(results(&s.search(&s.w.a, json!({"q": "granite"})).await).is_empty());
}

/// Scenario "A search at the time limit": a timeout of 1 ms and many parts to rank.
#[tokio::test]
async fn a_search_at_the_time_limit() {
    let mut s = s().await;
    let mut state = test_state(
        db::pool(config_in("strata_api", &s.w.db), 4, db::POOL_WAIT).unwrap(),
        Bounds::default(),
    );
    state.search_timeout = std::time::Duration::from_millis(1);
    s.w.app = strata::api::router(state);
    let (doc, _) = s.ready(s.w.a.id, None, vec![page(1, "slate")]).await;
    s.w.su
        .execute(
            "insert into text_parts (tenant_id, document_id, revision, part, location_kind, \
             location_number, part_text) \
             select $1, $2, 1, n + 1, 'page', 1, repeat('slate shale ', 400) \
             from generate_series(1, 20000) as n",
            &[&s.w.a.id, &doc],
        )
        .await
        .unwrap();
    let a = s.search(&s.w.a, json!({"q": "slate"})).await;
    assert_eq!(a.status, StatusCode::SERVICE_UNAVAILABLE, "{}", a.body);
    assert_eq!(a.headers["retry-after"], "1");
}

/// The reads and writes of the document tables in the database of the test, from the counts of
/// PostgreSQL. A server process reports its counts at the latest when it exits.
async fn document_ops(su: &tokio_postgres::Client) -> i64 {
    su.batch_execute("select pg_stat_clear_snapshot()")
        .await
        .unwrap();
    su.query_one(
        "select coalesce(sum(seq_scan + coalesce(idx_scan, 0) + n_tup_ins + n_tup_upd + n_tup_del), 0)::bigint \
         from pg_stat_user_tables \
         where relname in ('documents', 'revisions', 'text_parts', 'jobs', 'upload_slots')",
        &[],
    )
    .await
    .unwrap()
    .get(0)
}

/// Drops the router of the world and waits until each connection of `strata_api` to the
/// database has ended, so that its counts are in the statistics.
async fn close_api(s: &mut S) {
    s.w.app = axum::Router::new();
    for _ in 0..100 {
        let open: i64 = s
            .w
            .su
            .query_one(
                "select count(*) from pg_stat_activity where datname = $1 and usename = 'strata_api'",
                &[&s.w.db],
            )
            .await
            .unwrap()
            .get(0);
        if open == 0 {
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    panic!("the connections of strata_api did not end");
}

fn api_on(s: &S) -> axum::Router {
    let pool = db::pool(config_in("strata_api", &s.w.db), 4, db::POOL_WAIT).unwrap();
    strata::api::router(test_state(pool, Bounds::default()))
}

/// TC-4-03. An account that is not active gets `403` on each account action, and the API runs
/// no query on the document tables. A request of the account while it is active shows that the
/// counts see the reads of the routes.
#[tokio::test]
async fn tc_4_03_an_account_that_is_not_active_on_each_action() {
    let mut s = s().await;
    let carla = s.w.token("acme-corp");
    let doc = Uuid::from_u128(0x4003);
    let status = format!("/v1/documents/{doc}/revisions/1");
    s.w.app = api_on(&s);
    close_api(&mut s).await;
    let before = document_ops(&s.w.su).await;
    // The first request creates the account, and reads `revisions`.
    s.w.app = api_on(&s);
    assert_eq!(s.w.get(&carla, &status).await.status, StatusCode::NOT_FOUND);
    close_api(&mut s).await;
    let active = document_ops(&s.w.su).await;
    assert!(
        active > before,
        "the counts see the read of the status route"
    );

    s.w.su
        .execute(
            "update accounts set status = 'not-active' where subject = $1",
            &[&subject(&carla.token)],
        )
        .await
        .unwrap();
    s.w.app = api_on(&s);
    let start = json!({"file_name": "a.pdf", "content_type": "application/pdf", "size": 10});
    let entry = json!({"revisions": [{"document_id": doc.to_string(), "revision": 1}]});
    let answers = [
        s.w.start(&carla, None, start).await,
        s.w.finish(&carla, doc, 1).await,
        s.w.get(&carla, &status).await,
        s.w.query(&carla, "/v1/revisions", &entry).await,
        s.search(&carla, json!({"q": "anything"})).await,
    ];
    for (n, a) in answers.iter().enumerate() {
        assert_eq!(a.status, StatusCode::FORBIDDEN, "action {n}");
    }
    close_api(&mut s).await;
    assert_eq!(document_ops(&s.w.su).await, active, "no query on documents");
}

/// The `sub` of a token.
fn subject(token: &str) -> String {
    use base64::Engine;
    let payload = token.split('.').nth(1).unwrap();
    let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(payload)
        .unwrap();
    serde_json::from_slice::<Value>(&bytes).unwrap()["sub"]
        .as_str()
        .unwrap()
        .to_owned()
}
