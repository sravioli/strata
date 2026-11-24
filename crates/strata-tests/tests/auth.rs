//! L1 tests of the authentication of the API (spec request-authentication). The routes exist
//! only in this file: no design gives a public route that only authenticates.

mod common;

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use axum::body::{Body, to_bytes};
use axum::extract::{FromRef, State};
use axum::http::{HeaderMap, Request, StatusCode};
use axum::routing::{any, get};
use axum::{Json, Router};
use base64::Engine;
use common::*;
use jsonwebtoken::jwk::{Jwk, JwkSet};
use jsonwebtoken::{Algorithm, EncodingKey, Header};
use serde_json::{Value, json};
use strata::auth::keys::KeyCache;
use strata::auth::{Auth, Caller, Reject};
use strata::db::{self, Pool, tenant_context};
use strata_core::authorization::{Action, authorized_scope};
use strata_dev::{DevKey, KID};
use tokio_postgres::Client;
use tower::ServiceExt;
use uuid::Uuid;

// ---- A JWKS server that counts its requests and can stop responding.

#[derive(Clone, Default)]
struct Jwks {
    body: Arc<Mutex<Value>>,
    hits: Arc<AtomicUsize>,
    down: Arc<AtomicBool>,
}

impl Jwks {
    async fn serve(body: Value) -> (Self, String) {
        let s = Jwks {
            body: Arc::new(Mutex::new(body)),
            ..Jwks::default()
        };
        let app = Router::new().route(
            "/jwks.json",
            get(|State(s): State<Jwks>| async move {
                s.hits.fetch_add(1, Ordering::SeqCst);
                if s.down.load(Ordering::SeqCst) {
                    return Err(StatusCode::SERVICE_UNAVAILABLE);
                }
                Ok(Json(s.body.lock().unwrap().clone()))
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}/jwks.json", listener.local_addr().unwrap());
        tokio::spawn(axum::serve(listener, app.with_state(s.clone())).into_future());
        (s, url)
    }

    fn hits(&self) -> usize {
        self.hits.load(Ordering::SeqCst)
    }
}

// ---- The router of the tests: one route that shows the tenant of the caller.

#[derive(Clone)]
struct TestState {
    auth: Auth,
    handled: Arc<AtomicUsize>,
}

impl FromRef<TestState> for Auth {
    fn from_ref(s: &TestState) -> Auth {
        s.auth.clone()
    }
}

/// Decides with `authorized_scope`, opens the tenant transaction, and returns its tenant and
/// the body, which goes to PostgreSQL as a parameter.
async fn whoami(
    State(s): State<TestState>,
    Caller(id): Caller,
    body: String,
) -> Result<Json<Value>, Reject> {
    s.handled.fetch_add(1, Ordering::SeqCst);
    let scope = authorized_scope(id.principal(), Action::Search);
    let mut tx = id.into_tenant(scope).await?.ok_or(Reject::Forbidden)?;
    let (tenant, text) = tenant_context(&mut tx, &body).await?;
    tx.commit().await?;
    Ok(Json(json!({"tenant": tenant.to_string(), "text": text})))
}

struct Api {
    app: Router,
    handled: Arc<AtomicUsize>,
}

impl Api {
    fn new(pool: Pool, keys: KeyCache) -> Self {
        let handled = Arc::new(AtomicUsize::new(0));
        let state = TestState {
            auth: Auth::new(pool, Arc::new(keys)),
            handled: handled.clone(),
        };
        let app = Router::new()
            .route("/whoami", any(whoami))
            .route("/whoami/{tenant_id}", any(whoami))
            .with_state(state);
        Api { app, handled }
    }

    async fn call(&self, req: Request<Body>) -> (StatusCode, HeaderMap, Vec<u8>) {
        let resp = self.app.clone().oneshot(req).await.unwrap();
        let (parts, body) = resp.into_parts();
        (
            parts.status,
            parts.headers,
            to_bytes(body, 1 << 20).await.unwrap().to_vec(),
        )
    }

    async fn get(&self, token: &str) -> (StatusCode, HeaderMap, Vec<u8>) {
        self.call(
            Request::get("/whoami")
                .header("authorization", format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
    }

    /// The tenant that the API acted for, or the status of the rejection.
    async fn tenant(&self, token: &str) -> Result<Uuid, StatusCode> {
        let (status, _, body) = self.get(token).await;
        if status != StatusCode::OK {
            return Err(status);
        }
        let v: Value = serde_json::from_slice(&body).unwrap();
        Ok(v["tenant"].as_str().unwrap().parse().unwrap())
    }
}

// ---- Issuers, keys and tokens.

/// A registered EdDSA issuer with its key, its JWKS server and one tenant bound to `tenant_value`.
struct Issuer {
    id: Uuid,
    name: String,
    key: DevKey,
    jwks: Jwks,
}

async fn issuer_ed(su: &Client) -> Issuer {
    let key = DevKey::generate();
    let (jwks, url) = Jwks::serve(key.jwks()).await;
    let (id, name) = issuer(su, &url).await;
    Issuer {
        id,
        name,
        key,
        jwks,
    }
}

fn now() -> i64 {
    strata_dev::now()
}

fn claims(iss: &str, org: &str, sub: &str) -> Value {
    json!({"iss": iss, "aud": "strata", "org": org, "sub": sub, "iat": now() - 5, "exp": now() + 300})
}

impl Issuer {
    fn token(&self, org: &str, sub: &str) -> String {
        self.key.sign(&claims(&self.name, org, sub))
    }

    fn token_with(&self, change: impl FnOnce(&mut serde_json::Map<String, Value>)) -> String {
        let mut c = claims(&self.name, "acme-corp", "u-1");
        change(c.as_object_mut().unwrap());
        self.key.sign(&c)
    }
}

/// Tenant A, bound to `acme-corp` of a new EdDSA issuer.
async fn setup() -> (Client, Uuid, Issuer, Api) {
    let su = connect(&superuser()).await;
    let a = tenant(&su).await;
    let p1 = issuer_ed(&su).await;
    bind(&su, p1.id, "acme-corp", a).await;
    (su, a, p1, Api::new(api_pool(), KeyCache::new()))
}

fn b64(v: &[u8]) -> String {
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(v)
}

fn unsigned(header: Value, claims: &Value) -> String {
    format!(
        "{}.{}.",
        b64(header.to_string().as_bytes()),
        b64(claims.to_string().as_bytes())
    )
}

/// A change to the claims of a valid token.
type Change = Box<dyn FnOnce(&mut serde_json::Map<String, Value>)>;

// ---- The test cases.

/// TC-21-01
#[tokio::test]
async fn a_bound_tenant_value_gives_its_tenant() {
    let (_su, a, p1, api) = setup().await;
    assert_eq!(api.tenant(&p1.token("acme-corp", "u-1")).await, Ok(a));
}

/// TC-21-02
#[tokio::test]
async fn a_binding_of_a_different_issuer_does_not_apply() {
    let (su, _a, p1, api) = setup().await;
    let b = tenant(&su).await;
    let p2 = issuer_ed(&su).await;
    bind(&su, p2.id, "t-2", b).await;
    assert_eq!(api.tenant(&p2.token("t-2", "u-1")).await, Ok(b));
    assert_eq!(
        api.tenant(&p1.token("t-2", "u-1")).await,
        Err(StatusCode::UNAUTHORIZED)
    );
}

/// TC-21-03
#[tokio::test]
async fn a_token_of_an_issuer_that_is_not_registered_fails() {
    let (_su, _a, _p1, api) = setup().await;
    let stranger = DevKey::generate();
    let token = stranger.sign(&claims(
        &unique("https://stranger.example"),
        "acme-corp",
        "u-1",
    ));
    assert_eq!(api.tenant(&token).await, Err(StatusCode::UNAUTHORIZED));
}

/// An ES256 key pair, as PKCS#8 DER and its JWKS.
fn es256() -> (EncodingKey, Value, Jwk) {
    let mut seed = [0u8; 32];
    getrandom::fill(&mut seed).unwrap();
    let sk = p256::SecretKey::from_slice(&seed).unwrap();
    let der = p256::pkcs8::EncodePrivateKey::to_pkcs8_der(&sk).unwrap();
    let ek = EncodingKey::from_ec_der(der.as_bytes());
    let mut jwk = Jwk::from_encoding_key(&ek, Algorithm::ES256).unwrap();
    jwk.common.key_id = Some(KID.into());
    let set = serde_json::to_value(JwkSet {
        keys: vec![jwk.clone()],
    })
    .unwrap();
    (ek, set, jwk)
}

/// TC-21-04
#[tokio::test]
async fn a_different_algorithm_fails() {
    let su = connect(&superuser()).await;
    let a = tenant(&su).await;
    let (ek, set, jwk) = es256();
    let (_jwks, url) = Jwks::serve(set).await;
    let (id, name) = issuer_with(&su, &url, "ES256").await;
    bind(&su, id, "acme-corp", a).await;
    let api = Api::new(api_pool(), KeyCache::new());
    let c = claims(&name, "acme-corp", "u-1");
    let mut h = Header::new(Algorithm::ES256);
    h.kid = Some(KID.into());
    assert_eq!(
        api.tenant(&jsonwebtoken::encode(&h, &c, &ek).unwrap())
            .await,
        Ok(a),
        "ES256 passes"
    );

    let none = unsigned(json!({"alg": "none", "kid": KID}), &c);
    let mut hs = Header::new(Algorithm::HS256);
    hs.kid = Some(KID.into());
    let public = serde_json::to_vec(&jwk).unwrap();
    let hs256 = jsonwebtoken::encode(&hs, &c, &EncodingKey::from_secret(&public).unwrap()).unwrap();
    let rs256 = format!(
        "{}{}",
        unsigned(json!({"alg": "RS256", "kid": KID}), &c),
        b64(b"signature")
    );
    for (what, token) in [("none", none), ("HS256", hs256), ("RS256", rs256)] {
        assert_eq!(
            api.tenant(&token).await,
            Err(StatusCode::UNAUTHORIZED),
            "{what}"
        );
    }
}

/// TC-21-05
#[tokio::test]
async fn claims_of_time_and_audience() {
    let (_su, _a, p1, api) = setup().await;
    // Each case reads the clock when it builds its token, so a slow run does not move the bounds.
    // The future cases are 120 s ahead, not 61: both clocks count whole seconds, so at 61 a tick
    // between the token and the check lands on the edge of the 60 s leeway. The L0 tests of
    // `claims::check` test the edge itself with a fixed clock.
    let cases: Vec<(&str, Change)> = vec![
        (
            "aud",
            Box::new(|c| drop(c.insert("aud".into(), json!("other")))),
        ),
        ("no exp", Box::new(|c| drop(c.remove("exp")))),
        ("no iat", Box::new(|c| drop(c.remove("iat")))),
        (
            "exp past",
            Box::new(|c| drop(c.insert("exp".into(), json!(now() - 61)))),
        ),
        (
            "nbf future",
            Box::new(|c| drop(c.insert("nbf".into(), json!(now() + 120)))),
        ),
        (
            "iat future",
            Box::new(|c| drop(c.insert("iat".into(), json!(now() + 120)))),
        ),
        (
            "lifetime",
            Box::new(|c| drop(c.insert("exp".into(), json!(now() - 5 + 3601)))),
        ),
    ];
    for (what, change) in cases {
        assert_eq!(
            api.tenant(&p1.token_with(change)).await,
            Err(StatusCode::UNAUTHORIZED),
            "{what}"
        );
    }
}

/// TC-21-06
#[tokio::test]
async fn claims_of_tenant_and_account() {
    let (_su, _a, p1, api) = setup().await;
    for claim in ["org", "sub"] {
        assert_eq!(
            api.tenant(&p1.token_with(|c| drop(c.remove(claim)))).await,
            Err(StatusCode::UNAUTHORIZED)
        );
        for bad in [
            json!(""),
            json!(["acme-corp"]),
            json!(7),
            json!("x".repeat(257)),
        ] {
            let token = p1.token_with(|c| drop(c.insert(claim.into(), bad.clone())));
            assert_eq!(
                api.tenant(&token).await,
                Err(StatusCode::UNAUTHORIZED),
                "{claim}: {bad}"
            );
        }
    }
}

async fn accounts_of(su: &Client, tenant: Uuid, sub: &str) -> i64 {
    su.query_one(
        "select count(*) from accounts where tenant_id = $1 and subject = $2",
        &[&tenant, &sub],
    )
    .await
    .unwrap()
    .get(0)
}

/// TC-21-07
#[tokio::test]
async fn two_first_requests_give_one_account() {
    let (su, a, p1, api) = setup().await;
    let sub = unique("u");
    let token = p1.token("acme-corp", &sub);
    let (r1, r2) = tokio::join!(api.tenant(&token), api.tenant(&token));
    assert_eq!((r1, r2), (Ok(a), Ok(a)));
    assert_eq!(accounts_of(&su, a, &sub).await, 1);
}

/// TC-21-08
#[tokio::test]
async fn one_subject_of_two_issuers_gives_two_accounts() {
    let (su, a, p1, api) = setup().await;
    let p2 = issuer_ed(&su).await;
    bind(&su, p2.id, "acme-corp", a).await;
    let sub = unique("u");
    assert_eq!(api.tenant(&p1.token("acme-corp", &sub)).await, Ok(a));
    assert_eq!(api.tenant(&p2.token("acme-corp", &sub)).await, Ok(a));
    assert_eq!(accounts_of(&su, a, &sub).await, 2);
}

/// TC-21-09
#[tokio::test]
async fn a_change_by_the_operator_applies_to_the_next_request() {
    let (su, a, p1, api) = setup().await;
    let token = p1.token("acme-corp", "u-1");
    assert_eq!(api.tenant(&token).await, Ok(a));
    su.execute(
        "delete from issuer_bindings where issuer_id = $1",
        &[&p1.id],
    )
    .await
    .unwrap();
    assert_eq!(
        api.tenant(&token).await,
        Err(StatusCode::UNAUTHORIZED),
        "binding removed"
    );

    bind(&su, p1.id, "acme-corp", a).await;
    assert_eq!(api.tenant(&token).await, Ok(a));
    su.execute(
        "update issuers set status = 'not-active' where issuer_id = $1",
        &[&p1.id],
    )
    .await
    .unwrap();
    assert_eq!(
        api.tenant(&token).await,
        Err(StatusCode::UNAUTHORIZED),
        "issuer not active"
    );
}

/// TC-21-10
#[tokio::test]
async fn unknown_key_identifiers_cause_at_most_one_fetch() {
    let (_su, a, p1, api) = setup().await;
    assert_eq!(api.tenant(&p1.token("acme-corp", "u-1")).await, Ok(a));
    let before = p1.jwks.hits();
    for i in 0..100 {
        let mut h = Header::new(Algorithm::EdDSA);
        h.kid = Some(format!("unknown-{i}"));
        let token = jsonwebtoken::encode(
            &h,
            &claims(&p1.name, "acme-corp", "u-1"),
            &p1.key.encoding_key(),
        )
        .unwrap();
        assert_eq!(api.tenant(&token).await, Err(StatusCode::UNAUTHORIZED));
    }
    assert!(
        p1.jwks.hits() - before <= 1,
        "{} fetches",
        p1.jwks.hits() - before
    );
}

/// TC-21-11
#[tokio::test]
async fn a_failed_fetch_keeps_the_keys() {
    let su = connect(&superuser()).await;
    let a = tenant(&su).await;
    let p1 = issuer_ed(&su).await;
    bind(&su, p1.id, "acme-corp", a).await;
    // A refresh before each request, so that each request tries a fetch.
    let api = Api::new(
        api_pool(),
        KeyCache::with_times(Duration::ZERO, Duration::ZERO),
    );
    let token = p1.token("acme-corp", "u-1");
    assert_eq!(api.tenant(&token).await, Ok(a));
    p1.jwks.down.store(true, Ordering::SeqCst);
    let before = p1.jwks.hits();
    assert_eq!(api.tenant(&token).await, Ok(a), "the keys that it has");
    assert!(p1.jwks.hits() > before, "the API tried a fetch");

    let p3 = issuer_ed(&su).await;
    p3.jwks.down.store(true, Ordering::SeqCst);
    bind(&su, p3.id, "acme-corp", a).await;
    assert_eq!(
        api.tenant(&p3.token("acme-corp", "u-1")).await,
        Err(StatusCode::UNAUTHORIZED),
        "never fetched"
    );
}

/// TC-21-12
#[tokio::test]
async fn a_key_in_the_header_is_not_used() {
    let (_su, _a, p1, api) = setup().await;
    let attacker = DevKey::generate();
    let (evil, evil_url) = Jwks::serve(attacker.jwks()).await;
    let evil_jwk: Jwk = serde_json::from_value(attacker.jwks()["keys"][0].clone()).unwrap();
    let c = claims(&p1.name, "acme-corp", "u-1");
    let base = || {
        let mut h = Header::new(Algorithm::EdDSA);
        h.kid = Some(KID.into());
        h
    };
    let mut headers = Vec::new();
    let mut h = base();
    h.jku = Some(evil_url.clone());
    headers.push(("jku", h));
    let mut h = base();
    h.jwk = Some(evil_jwk);
    headers.push(("jwk", h));
    let mut h = base();
    h.x5u = Some(evil_url.clone());
    headers.push(("x5u", h));
    let mut h = base();
    h.x5c = Some(vec![b64(b"not a certificate")]);
    headers.push(("x5c", h));
    for (what, h) in headers {
        let token = jsonwebtoken::encode(&h, &c, &attacker.encoding_key()).unwrap();
        assert_eq!(
            api.tenant(&token).await,
            Err(StatusCode::UNAUTHORIZED),
            "{what}"
        );
    }
    assert_eq!(evil.hits(), 0, "no request to the URL of the header");
}

/// A writer for the request log of one test.
#[derive(Clone, Default)]
struct Log(Arc<Mutex<Vec<u8>>>);

impl std::io::Write for Log {
    fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(b);
        Ok(b.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

/// TC-21-13
#[tokio::test]
async fn the_actor_is_in_the_log_and_the_response_does_not_change() {
    let (su, _a, p1, api) = setup().await;
    let log = Log::default();
    let writer = log.clone();
    let subscriber = tracing_subscriber::fmt()
        .with_writer(move || writer.clone())
        .with_ansi(false)
        .finish();
    let _guard = tracing::subscriber::set_default(subscriber);
    let actor = unique("p1-backend");
    let with_act = p1.token_with(|c| drop(c.insert("act".into(), json!({"sub": actor}))));
    let plain = p1.token("acme-corp", "u-1");
    let (s1, _, b1) = api.get(&with_act).await;
    let (s2, _, b2) = api.get(&plain).await;
    assert_eq!((s1, b1), (s2, b2));
    let text = String::from_utf8(log.0.lock().unwrap().clone()).unwrap();
    // The account ID of the caller, not `act.sub` (0021, amended in #133).
    let account: Uuid = su
        .query_one(
            "select account_id from accounts where issuer_id = $1 and subject = 'u-1'",
            &[&p1.id],
        )
        .await
        .unwrap()
        .get(0);
    assert!(
        text.contains(&format!("account_id={account}")) && text.contains("request for an actor"),
        "{text}"
    );
    assert!(!text.contains(&actor), "{text}");
}

/// TC-21-14 and TC-21-16
#[tokio::test]
async fn each_rejection_of_a_token_gives_the_same_response() {
    let (su, _a, p1, api) = setup().await;
    let fresh = issuer_ed(&su).await;
    let large = fresh.token_with(|c| drop(c.insert("pad".into(), json!("x".repeat(8 * 1024)))));
    let mut no_kid = Header::new(Algorithm::EdDSA);
    no_kid.kid = None;
    let unbound = issuer_ed(&su).await;
    let stopped = issuer_ed(&su).await;
    su.execute(
        "update issuers set status = 'not-active' where issuer_id = $1",
        &[&stopped.id],
    )
    .await
    .unwrap();
    let tokens = [
        ("too large", large),
        ("not a token", "abc".into()),
        (
            "no kid",
            jsonwebtoken::encode(
                &no_kid,
                &claims(&p1.name, "acme-corp", "u"),
                &p1.key.encoding_key(),
            )
            .unwrap(),
        ),
        (
            "unknown issuer",
            DevKey::generate().sign(&claims("nobody", "acme-corp", "u")),
        ),
        ("issuer not active", stopped.token("acme-corp", "u")),
        (
            "bad signature",
            DevKey::generate().sign(&claims(&p1.name, "acme-corp", "u")),
        ),
        ("bad claims", p1.token_with(|c| drop(c.remove("exp")))),
        ("no binding", unbound.token("acme-corp", "u")),
    ];
    let mut responses = Vec::new();
    for (what, token) in &tokens {
        let (status, headers, body) = api.get(token).await;
        responses.push((what, status, headers.get("www-authenticate").cloned(), body));
    }
    let (_, s, h, b) = &responses[0];
    assert_eq!(
        (*s, h.as_ref().map(|v| v.to_str().unwrap()), b.len()),
        (StatusCode::UNAUTHORIZED, Some("Bearer"), 0)
    );
    for r in &responses {
        assert_eq!((r.1, &r.2, &r.3), (*s, h, b), "{}", r.0);
    }
    let (status, headers, body) = api
        .call(Request::get("/whoami").body(Body::empty()).unwrap())
        .await;
    assert_eq!(
        (status, headers.get("www-authenticate"), body.len()),
        (*s, h.as_ref(), 0),
        "no header"
    );
    // TC-21-16: the large token is otherwise valid, and the API did not parse it: it fetched no key.
    assert_eq!(fresh.jwks.hits(), 0);
}

/// TC-3-09
#[tokio::test]
async fn a_tenant_that_is_not_active() {
    let (su, a, p1, api) = setup().await;
    let token = p1.token("acme-corp", "u-1");
    assert_eq!(api.tenant(&token).await, Ok(a));
    su.execute(
        "update tenants set status = 'not-active' where tenant_id = $1",
        &[&a],
    )
    .await
    .unwrap();
    let handled = api.handled.load(Ordering::SeqCst);
    let (status, _, body) = api.get(&token).await;
    assert_eq!((status, body.len()), (StatusCode::FORBIDDEN, 0));
    assert_eq!(
        api.handled.load(Ordering::SeqCst),
        handled,
        "the route ran no query"
    );
}

/// TC-4-04
#[tokio::test]
async fn an_account_that_the_operator_stops() {
    let (su, a, p1, api) = setup().await;
    let sub = unique("u");
    let token = p1.token("acme-corp", &sub);
    assert_eq!(api.tenant(&token).await, Ok(a));
    let op = connect(&config("strata_operator", POSTGRES)).await;
    op.execute(
        "update accounts set status = 'not-active' where tenant_id = $1 and subject = $2",
        &[&a, &sub],
    )
    .await
    .unwrap();
    assert_eq!(api.tenant(&token).await, Err(StatusCode::FORBIDDEN));
    drop(su);
}

/// TC-3-11
#[tokio::test]
async fn a_tenant_in_the_request_is_ignored() {
    let (su, a, p1, api) = setup().await;
    let b = tenant(&su).await.to_string();
    let auth = format!("Bearer {}", p1.token("acme-corp", "u-1"));
    let requests = [
        Request::get("/whoami")
            .header("x-tenant-id", &b)
            .header("authorization", &auth)
            .body(Body::empty()),
        Request::get(format!("/whoami?tenant_id={b}"))
            .header("authorization", &auth)
            .body(Body::empty()),
        Request::get(format!("/whoami/{b}"))
            .header("authorization", &auth)
            .body(Body::empty()),
        Request::post("/whoami")
            .header("authorization", &auth)
            .body(Body::from(json!({"tenant_id": b}).to_string())),
    ];
    for req in requests {
        let (status, _, body) = api.call(req.unwrap()).await;
        assert_eq!(status, StatusCode::OK);
        let v: Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(v["tenant"], json!(a.to_string()));
    }
}

/// TC-3-16
#[tokio::test]
async fn sql_text_in_a_parameter_stays_data() {
    let (su, a, p1, api) = setup().await;
    let b = tenant(&su).await;
    let attack = format!("x'); select set_config('app.tenant_id', '{b}', true); --");
    let req = Request::post("/whoami")
        .header(
            "authorization",
            format!("Bearer {}", p1.token("acme-corp", "u-1")),
        )
        .body(Body::from(attack.clone()))
        .unwrap();
    let (status, _, body) = api.call(req).await;
    assert_eq!(status, StatusCode::OK);
    let v: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(
        (v["tenant"].clone(), v["text"].clone()),
        (json!(a.to_string()), json!(attack))
    );
}

/// TC-3-13. The method: a new database in which `strata_api` has no grant on a tenant-owned
/// table. A query on one of them there fails, and the API answers 500. A 401 thus shows that the
/// API ran no such query. The first request, a valid token, shows that the method sees a query.
#[tokio::test]
async fn a_changed_or_expired_token_runs_no_tenant_query() {
    migrated();
    let su = connect(&superuser()).await;
    let name = unique("strata_probe").replace('-', "_");
    su.batch_execute(&format!("create database {name} owner strata_owner"))
        .await
        .unwrap();
    let mut owner = config("strata_owner", POSTGRES);
    owner.dbname(&name);
    strata::migrate::run(&owner).await.unwrap();
    let mut probe_su = superuser();
    probe_su.dbname(&name);
    let psu = connect(&probe_su).await;
    psu.batch_execute("revoke all on tenants, accounts, issuer_bindings from strata_api")
        .await
        .unwrap();
    let a = tenant(&psu).await;
    let b = tenant(&psu).await;
    let p1 = issuer_ed(&psu).await;
    bind(&psu, p1.id, "acme-corp", a).await;
    bind(&psu, p1.id, "globex", b).await;
    let mut api_cfg = config("strata_api", POSTGRES);
    api_cfg.dbname(&name);
    let api = Api::new(
        db::pool(api_cfg, 2, db::POOL_WAIT).unwrap(),
        KeyCache::new(),
    );

    let valid = p1.token("acme-corp", "u-1");
    assert_eq!(
        api.tenant(&valid).await,
        Err(StatusCode::INTERNAL_SERVER_ERROR),
        "the probe sees a tenant query"
    );

    // The tenant changed in the payload, with the signature of the first payload.
    let parts: Vec<&str> = valid.split('.').collect();
    let changed = claims(&p1.name, "globex", "u-1");
    let changed = format!(
        "{}.{}.{}",
        parts[0],
        b64(changed.to_string().as_bytes()),
        parts[2]
    );
    let broken = format!("{}.{}.{}", parts[0], parts[1], b64(b"broken"));
    let other_alg = format!(
        "{}{}",
        unsigned(
            json!({"alg": "ES256", "kid": KID}),
            &claims(&p1.name, "acme-corp", "u")
        ),
        b64(b"s")
    );
    let expired = p1.token_with(|c| drop(c.insert("exp".into(), json!(now() - 120))));
    for (what, token) in [
        ("changed tenant", changed),
        ("broken signature", broken),
        ("other algorithm", other_alg),
        ("expired", expired),
    ] {
        assert_eq!(
            api.tenant(&token).await,
            Err(StatusCode::UNAUTHORIZED),
            "{what}"
        );
    }
    drop((psu, api));
    su.batch_execute(&format!("drop database {name} with (force)"))
        .await
        .unwrap();
}

/// TC-2-06
#[tokio::test]
async fn the_wait_for_a_connection_has_a_limit() {
    let (_su, _a, p1, _api) = setup().await;
    let pool = db::pool(config("strata_api", PGBOUNCER), 1, db::POOL_WAIT).unwrap();
    let api = Api::new(pool.clone(), KeyCache::new());
    let held = pool.get().await.unwrap();
    let start = Instant::now();
    let (status, headers, _) = api.get(&p1.token("acme-corp", "u-1")).await;
    let waited = start.elapsed();
    drop(held);
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(headers.get("retry-after").unwrap(), "1");
    assert!(
        waited >= Duration::from_secs(2) && waited < Duration::from_secs(4),
        "{waited:?}"
    );
}

/// The development issuer works with the API (spec dev-issuer, "The API accepts the token").
#[tokio::test]
async fn the_api_accepts_a_token_of_the_development_issuer() {
    let su = connect(&superuser()).await;
    let a = tenant(&su).await;
    let key = DevKey::generate();
    let (_jwks, url) = Jwks::serve(key.jwks()).await;
    let id: Uuid = su
        .query_one(
            "insert into issuers (issuer, jwks_url, audience, algorithm, tenant_claim) \
             values ($1, $2, $3, 'EdDSA', $4) \
             on conflict (issuer) do update set jwks_url = excluded.jwks_url, status = 'active' \
             returning issuer_id",
            &[
                &strata_dev::ISSUER,
                &url,
                &strata_dev::AUDIENCE,
                &strata_dev::TENANT_CLAIM,
            ],
        )
        .await
        .unwrap()
        .get(0);
    let value = unique("acme");
    bind(&su, id, &value, a).await;
    let api = Api::new(api_pool(), KeyCache::new());
    assert_eq!(api.tenant(&key.token(&value, "alice", 60)).await, Ok(a));
    let alice: i64 = su
        .query_one("select count(*) from accounts where tenant_id = $1 and issuer_id = $2 and subject = 'alice'",
                   &[&a, &id])
        .await
        .unwrap()
        .get(0);
    assert_eq!(alice, 1);
}
