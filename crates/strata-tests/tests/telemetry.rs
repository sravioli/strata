//! L1 tests of the spec observability (tech design 0133). The in-memory exporters of
//! `telemetry::for_tests` hold the spans, the log records and the metrics of the router and the
//! worker. The processes of `strata api` and `strata worker` show the switch and the stop.
//! Each test runs in a new database: a worker claims any due job.

mod common;

use std::io::Read;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use axum::body::Body;
use axum::http::{Request, StatusCode};
use common::api::*;
use common::*;
use serde_json::{Value, json};
use strata::api::{AppState, router};
use strata::db;
use strata::telemetry::{ForTests, SpanData, for_tests};
use strata::upload::Bounds;
use strata::worker::{SETTINGS, Worker};
use uuid::Uuid;

const FIXTURE: &str = "two-pages.pdf";
/// A word of page 2 of the fixture. No code or log text of Strata holds it.
const WORD: &str = "bravo";

/// A world in a new database. Its router and its worker record to `t`.
async fn world(t: &ForTests) -> (World, Worker) {
    let mut w = world_in_new_database(Bounds::default()).await;
    let pool = db::pool(config_in("strata_api", &w.db), 4, db::POOL_WAIT).unwrap();
    w.app = router(AppState {
        metrics: t.metrics.clone(),
        ..test_state(pool, Bounds::default())
    });
    let worker = Worker {
        pool: db::pool(config_in("strata_worker", &w.db), 4, db::POOL_WAIT).unwrap(),
        storage: storage(),
        sandbox: sandbox(&strata_extract(), None, strata::sandbox::LIMITS),
        settings: SETTINGS,
        metrics: t.metrics.clone(),
    };
    (w, worker)
}

fn fixture() -> Vec<u8> {
    std::fs::read(format!("{}/fixtures/{FIXTURE}", env!("CARGO_MANIFEST_DIR"))).unwrap()
}

fn start_body(size: usize) -> Value {
    json!({"file_name": "a.pdf", "content_type": "application/pdf", "size": size})
}

/// The upload of the fixture through the routes: the document and the signed URL.
async fn upload(w: &World, t: &Tenant) -> (Uuid, String) {
    let bytes = fixture();
    let a = w.start(t, None, start_body(bytes.len())).await;
    assert_eq!(a.status, StatusCode::CREATED, "{}", a.body);
    let d: Uuid = a.body["document_id"].as_str().unwrap().parse().unwrap();
    let url = a.body["upload_url"].as_str().unwrap().to_owned();
    assert_eq!(
        put(&url.parse().unwrap(), &bytes, bytes.len(), Duration::ZERO).await,
        200
    );
    assert_accepted(&w.finish(t, d, 1).await, d, 1);
    (d, url)
}

fn named<'a>(spans: &'a [SpanData], name: &str) -> Vec<&'a SpanData> {
    spans.iter().filter(|s| s.name == name).collect()
}

fn one<'a>(spans: &'a [SpanData], name: &str) -> &'a SpanData {
    let found = named(spans, name);
    let names: Vec<_> = spans.iter().map(|s| s.name.to_string()).collect();
    assert_eq!(found.len(), 1, "one span {name} in {names:?}");
    found[0]
}

fn attr(s: &SpanData, key: &str) -> Option<String> {
    s.attributes
        .iter()
        .find(|kv| kv.key.as_str() == key)
        .map(|kv| kv.value.to_string())
}

/// The parent span ID of a root span.
const ROOT: &str = "0000000000000000";

const FINISH: &str = "POST /v1/documents/{document_id}/revisions/{revision}/finish";

/// Scenario "A traceparent header of a client".
#[tokio::test]
async fn a_traceparent_header_of_a_client_starts_no_trace() {
    let t = for_tests();
    let _g = tracing::dispatcher::set_default(&t.dispatch);
    let (w, _) = world(&t).await;
    let header = "00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01";
    let req = Request::builder()
        .method("QUERY")
        .uri("/v1/search")
        .header("authorization", format!("Bearer {}", w.a.token))
        .header("content-type", "application/json")
        .header("traceparent", header)
        .body(Body::from(json!({"q": "alpha", "limit": 5}).to_string()))
        .unwrap();
    assert_eq!(w.call(req).await.status, StatusCode::OK);
    t.flush();
    let spans = t.spans();
    let s = one(&spans, "QUERY /v1/search");
    assert_ne!(
        s.span_context.trace_id().to_string(),
        "4bf92f3577b34da6a3ce929d0e0e4736"
    );
    assert_eq!(s.parent_span_id.to_string(), ROOT, "a root span");
    assert_eq!(attr(s, "http.route").as_deref(), Some("/v1/search"));
    assert_eq!(attr(s, "http.response.status_code").as_deref(), Some("200"));
    assert_eq!(attr(s, "strata.tenant_id"), Some(w.a.id.to_string()));
    assert!(attr(s, "strata.account_id").is_some());
    // The length of `q` and the limit, never `q`.
    assert_eq!(attr(s, "strata.search.q_length").as_deref(), Some("5"));
    assert_eq!(attr(s, "strata.search.limit").as_deref(), Some("5"));
    assert!(!format!("{spans:?}").contains("alpha"));
}

/// TC-133-02, and the span of the query `write_finish` (task 4.2).
#[tokio::test]
async fn tc_133_02_the_finish_and_its_job_are_one_trace() {
    let t = for_tests();
    let _g = tracing::dispatcher::set_default(&t.dispatch);
    let (w, worker) = world(&t).await;
    let (d, _) = upload(&w, &w.a).await;
    assert!(worker.run_one().await.unwrap().is_some(), "the job runs");
    t.flush();
    let spans = t.spans();
    let traceparent: String =
        w.su.query_one("select traceparent from jobs where document_id = $1", &[&d])
            .await
            .unwrap()
            .get(0);
    let finish = one(&spans, FINISH);
    let trace = finish.span_context.trace_id();
    assert_eq!(
        traceparent,
        format!("00-{trace}-{}-01", finish.span_context.span_id())
    );
    let write = one(&spans, "write_finish");
    assert_eq!(write.parent_span_id, finish.span_context.span_id());
    assert_eq!(attr(write, "db.system.name").as_deref(), Some("postgresql"));
    let text = format!("{:?}", write.attributes).to_lowercase();
    assert!(
        !text.contains("insert") && !text.contains("update"),
        "{text}"
    );

    let job = one(&spans, "job ProcessDocument");
    let attempt = one(&spans, "attempt");
    let extraction = one(&spans, "extraction");
    assert_eq!(job.parent_span_id, finish.span_context.span_id());
    assert_eq!(attempt.parent_span_id, job.span_context.span_id());
    assert_eq!(extraction.parent_span_id, attempt.span_context.span_id());
    for s in [job, attempt, extraction] {
        assert_eq!(s.span_context.trace_id(), trace, "{}", s.name);
    }
    assert_eq!(attr(attempt, "strata.attempt").as_deref(), Some("1"));
    assert_eq!(attr(attempt, "strata.result").as_deref(), Some("Done"));
    assert_eq!(attr(extraction, "strata.format").as_deref(), Some("pdf"));
    assert_eq!(attr(extraction, "strata.exit_class").as_deref(), Some("Ok"));
    assert_eq!(attr(extraction, "strata.parts").as_deref(), Some("2"));
    assert!(attr(extraction, "strata.duration_s").is_some());
}

/// TC-133-03
#[tokio::test]
async fn tc_133_03_the_word_the_subject_and_the_url_stay_out() {
    let t = for_tests();
    let _g = tracing::dispatcher::set_default(&t.dispatch);
    let (w, worker) = world(&t).await;
    let (d, url) = upload(&w, &w.a).await;
    assert!(worker.run_one().await.unwrap().is_some(), "the job runs");
    let (sub, actor) = (unique("subject"), unique("actor"));
    let now = strata_dev::now();
    let claims = json!({"iss": w.issuer, "aud": "strata", "org": "acme-corp", "sub": sub,
                        "act": {"sub": actor}, "iat": now - 5, "exp": now + 300});
    let caller = Tenant {
        id: w.a.id,
        token: w._key.sign(&claims),
    };
    let found = w.query(&caller, "/v1/search", &json!({"q": WORD})).await;
    assert_eq!(found.status, StatusCode::OK, "{}", found.body);
    assert_eq!(found.body["results"][0]["document_id"], d.to_string());
    t.flush();
    let all = format!(
        "{:?}\n{:?}\n{:?}",
        t.spans(),
        t.logs.get_emitted_logs().unwrap(),
        t.metric_points()
    );
    assert!(
        all.contains("request for an actor"),
        "the actor event exists"
    );
    let signature = url.split("X-Amz-Signature=").nth(1).unwrap();
    for secret in [WORD, sub.as_str(), actor.as_str(), url.as_str(), signature] {
        assert!(!all.contains(secret), "{secret} is in the telemetry");
    }
}

/// TC-133-04
#[tokio::test]
async fn tc_133_04_no_id_in_a_metric() {
    let t = for_tests();
    let _g = tracing::dispatcher::set_default(&t.dispatch);
    let (w, worker) = world(&t).await;
    let mut ids = vec![w.a.id.to_string(), w.b.id.to_string()];
    for tenant in [&w.a, &w.b] {
        let (d, _) = upload(&w, tenant).await;
        ids.push(d.to_string());
        assert!(worker.run_one().await.unwrap().is_some(), "the job runs");
        assert_eq!(w.get(tenant, &location(d, 1)).await.status, StatusCode::OK);
        let batch = json!({"revisions": [{"document_id": d, "revision": 1}]});
        assert_eq!(
            w.query(tenant, "/v1/revisions", &batch).await.status,
            StatusCode::OK
        );
        let next = w.start(tenant, Some(d), start_body(10)).await;
        assert_eq!(next.status, StatusCode::CREATED, "{}", next.body);
        let search = w.query(tenant, "/v1/search", &json!({"q": "alpha"})).await;
        assert_eq!(search.status, StatusCode::OK);
    }
    for row in
        w.su.query("select account_id::text from accounts", &[])
            .await
            .unwrap()
    {
        ids.push(row.get(0));
    }
    t.flush();
    let points = t.metric_points();
    let names: std::collections::BTreeSet<_> = points.iter().map(|(n, _)| n.as_str()).collect();
    for n in [
        "http.server.request.duration",
        "strata.api.requests_in_flight",
        "strata.job.attempts",
        "strata.extraction.duration",
    ] {
        assert!(names.contains(n), "{n} in {names:?}");
    }
    let keys = [
        "http.request.method",
        "http.route",
        "http.response.status_code",
        "reason",
        "kind",
        "result",
        "format",
    ];
    for (name, attributes) in &points {
        for (k, v) in attributes {
            assert!(keys.contains(&k.as_str()), "{name}: the key {k}");
            assert!(!ids.iter().any(|id| v.contains(id)), "{name}: {k}={v}");
        }
    }
}

/// TC-133-05
#[tokio::test]
async fn tc_133_05_a_value_of_another_format_and_null() {
    let t = for_tests();
    let _g = tracing::dispatcher::set_default(&t.dispatch);
    let (w, worker) = world(&t).await;
    let (doc, _) = document(&w.su, w.a.id, "Uploaded").await;
    let insert = "insert into jobs (tenant_id, kind, document_id, revision, run_after, traceparent) \
                  values ($1, 'ProcessDocument', $2, 1, now() - interval '1 second', $3)";
    for wrong in [
        "",
        "01-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01",
        "00-4BF92F3577B34DA6A3CE929D0E0E4736-00f067aa0ba902b7-01",
        "00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7",
    ] {
        let e =
            w.su.execute(insert, &[&w.a.id, &doc, &wrong])
                .await
                .unwrap_err();
        assert_eq!(
            e.code(),
            Some(&tokio_postgres::error::SqlState::CHECK_VIOLATION),
            "{wrong}"
        );
    }
    w.su.execute(insert, &[&w.a.id, &doc, &None::<String>])
        .await
        .unwrap();
    assert!(worker.run_one().await.unwrap().is_some(), "the job runs");
    t.flush();
    let spans = t.spans();
    let job = one(&spans, "job ProcessDocument");
    assert_eq!(job.parent_span_id.to_string(), ROOT, "a root span");
}

/// Scenario "No metrics endpoint".
#[tokio::test]
async fn no_metrics_endpoint() {
    let w = world_in_new_database(Bounds::default()).await;
    let req = Request::builder()
        .uri("/metrics")
        .body(Body::empty())
        .unwrap();
    assert_eq!(w.call(req).await.status, StatusCode::NOT_FOUND);
}

// ---- The processes of `strata api` and `strata worker`.

/// The `strata` binary of the build of this test, as `strata_extract`.
fn strata_bin() -> PathBuf {
    let exe = std::env::current_exe().unwrap();
    exe.ancestors()
        .map(|a| a.join("strata"))
        .find(|p| p.is_file())
        .unwrap_or_else(|| panic!("build strata first ({})", exe.display()))
}

struct Process {
    child: Child,
    log: PathBuf,
}

impl Process {
    /// `strata <args>` with the variables of `vars`, the storage of compose.yaml, and no other
    /// variable of OpenTelemetry. stderr goes to a file.
    fn start(args: &[&str], db: &str, role: &str, vars: &[(&str, String)]) -> Process {
        use strata::storage::*;
        let log = std::env::temp_dir().join(format!("{}.log", unique("strata-proc")));
        let mut cmd = Command::new(strata_bin());
        cmd.args(args)
            .env_clear()
            .env("PATH", std::env::var("PATH").unwrap_or_default())
            .env("RUST_LOG", "info")
            .env(
                strata::DATABASE_URL,
                format!("postgres://{role}:{role}@localhost:{POSTGRES}/{db}"),
            )
            .env(ENDPOINT, "http://localhost:3900")
            .env(ACCESS_KEY, "GK000000000000000000000000")
            .env(
                SECRET_KEY,
                "0000000000000000000000000000000000000000000000000000000000000000",
            )
            .env(REGION, "garage")
            .env(BUCKET, "strata")
            .env(ALLOW_HTTP, "true")
            .envs(vars.iter().map(|(k, v)| (k, v)))
            .stdout(Stdio::null())
            .stderr(std::fs::File::create(&log).unwrap());
        // The coverage build writes its profile where the variable says.
        if let Ok(p) = std::env::var("LLVM_PROFILE_FILE") {
            cmd.env("LLVM_PROFILE_FILE", p);
        }
        Process {
            child: cmd.spawn().unwrap(),
            log,
        }
    }

    fn log(&self) -> String {
        let mut s = String::new();
        std::fs::File::open(&self.log)
            .unwrap()
            .read_to_string(&mut s)
            .unwrap();
        s
    }

    /// SIGTERM, the time to the exit, and the log. The exit is a success.
    fn stop(mut self) -> (Duration, String) {
        let start = Instant::now();
        // SAFETY: a signal to the child that this test started.
        unsafe { libc::kill(self.child.id() as i32, libc::SIGTERM) };
        loop {
            if let Some(status) = self.child.try_wait().unwrap() {
                let took = start.elapsed();
                assert!(status.success(), "{status}: {}", self.log());
                return (took, self.log());
            }
            assert!(
                start.elapsed() < Duration::from_secs(20),
                "no stop: {}",
                self.log()
            );
            std::thread::sleep(Duration::from_millis(20));
        }
    }
}

struct Services {
    api: Process,
    worker: Process,
    addr: String,
}

/// `strata api` and `strata worker` on the database of `w`, with `vars`.
async fn services(w: &World, vars: &[(&str, String)]) -> Services {
    use strata::sandbox::*;
    let addr = {
        let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        l.local_addr().unwrap().to_string()
    };
    let api = Process::start(&["api", "--listen", &addr], &w.db, "strata_api", vars);
    let nsjail = std::env::var(NSJAIL)
        .unwrap_or_else(|_| format!("{}/../../.tools/bin/nsjail", env!("CARGO_MANIFEST_DIR")));
    let mut worker_vars = vec![
        (NSJAIL, nsjail),
        (EXTRACT, strata_extract().display().to_string()),
        (
            CGROUP,
            std::env::var(CGROUP).expect("STRATA_CGROUP: run the L1 tests with make l1"),
        ),
    ];
    worker_vars.extend(vars.iter().cloned());
    let worker = Process::start(&["worker"], &w.db, "strata_worker", &worker_vars);
    let start = Instant::now();
    while tokio::net::TcpStream::connect(&addr).await.is_err() {
        assert!(start.elapsed() < Duration::from_secs(10), "{}", api.log());
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    Services { api, worker, addr }
}

/// One request over HTTP/1.1: the status and the JSON body.
async fn http(addr: &str, method: &str, path: &str, token: &str, body: &Value) -> (u16, Value) {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let body = body.to_string();
    let mut s = tokio::net::TcpStream::connect(addr).await.unwrap();
    let head = format!(
        "{method} {path} HTTP/1.1\r\nHost: {addr}\r\nAuthorization: Bearer {token}\r\n\
         Content-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    s.write_all(head.as_bytes()).await.unwrap();
    s.write_all(body.as_bytes()).await.unwrap();
    let mut resp = Vec::new();
    s.read_to_end(&mut resp).await.unwrap();
    let text = String::from_utf8_lossy(&resp);
    let status = text
        .split(' ')
        .nth(1)
        .and_then(|c| c.parse().ok())
        .unwrap_or(0);
    let json = text
        .split_once("\r\n\r\n")
        .and_then(|(_, b)| serde_json::from_str(b).ok())
        .unwrap_or(Value::Null);
    (status, json)
}

/// An upload, its job and a search through the processes. Each answer is the answer with
/// telemetry off.
async fn upload_and_search(w: &World, s: &Services) {
    let bytes = fixture();
    let token = &w.a.token;
    let (status, a) = http(
        &s.addr,
        "POST",
        "/v1/documents",
        token,
        &start_body(bytes.len()),
    )
    .await;
    assert_eq!(status, 201, "{a}");
    let d: Uuid = a["document_id"].as_str().unwrap().parse().unwrap();
    let url: url::Url = a["upload_url"].as_str().unwrap().parse().unwrap();
    assert_eq!(put(&url, &bytes, bytes.len(), Duration::ZERO).await, 200);
    let finish = format!("/v1/documents/{d}/revisions/1/finish");
    assert_eq!(
        http(&s.addr, "POST", &finish, token, &json!({})).await.0,
        202
    );
    let start = Instant::now();
    loop {
        let row =
            w.su.query_one("select state from jobs where document_id = $1", &[&d])
                .await
                .unwrap();
        if row.get::<_, &str>(0) == "Done" {
            break;
        }
        assert!(
            start.elapsed() < Duration::from_secs(30),
            "the job ends: {}",
            s.worker.log()
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    let (status, found) = http(&s.addr, "QUERY", "/v1/search", token, &json!({"q": WORD})).await;
    assert_eq!(status, 200, "{found}");
    assert_eq!(found["results"][0]["document_id"], d.to_string());
}

/// TC-133-01
#[tokio::test]
async fn tc_133_01_no_endpoint_no_telemetry() {
    let w = world_in_new_database(Bounds::default()).await;
    // A lure: the variables of each signal point here, but `OTEL_EXPORTER_OTLP_ENDPOINT` is not
    // set, so nothing connects.
    let lure = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    lure.set_nonblocking(true).unwrap();
    let at = format!("http://{}", lure.local_addr().unwrap());
    let vars = [
        (
            "OTEL_EXPORTER_OTLP_TRACES_ENDPOINT",
            format!("{at}/v1/traces"),
        ),
        ("OTEL_EXPORTER_OTLP_LOGS_ENDPOINT", format!("{at}/v1/logs")),
        (
            "OTEL_EXPORTER_OTLP_METRICS_ENDPOINT",
            format!("{at}/v1/metrics"),
        ),
    ];
    let s = services(&w, &vars).await;
    upload_and_search(&w, &s).await;
    let (api, worker) = (s.api.log(), s.worker.log());
    s.api.stop();
    s.worker.stop();
    assert!(lure.accept().is_err(), "no connection for telemetry");
    // The stderr log of before 0133.
    assert!(api.contains("strata api") && api.contains("INFO"), "{api}");
    assert!(worker.contains("a job ended"), "{worker}");
}

/// TC-133-06
#[tokio::test]
async fn tc_133_06_the_collector_does_not_listen() {
    let w = world_in_new_database(Bounds::default()).await;
    let dead = {
        let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        format!("http://{}", l.local_addr().unwrap())
    };
    let s = services(&w, &[(strata::telemetry::ENDPOINT, dead)]).await;
    upload_and_search(&w, &s).await;
    for (name, p) in [("api", s.api), ("worker", s.worker)] {
        let (took, log) = p.stop();
        assert!(took < Duration::from_secs(5), "{name} stops in {took:?}");
        // The flush at the stop fails, and says so on stderr.
        assert!(
            log.contains("ERROR") || log.contains("telemetry:"),
            "{name}: {log}"
        );
    }
}
