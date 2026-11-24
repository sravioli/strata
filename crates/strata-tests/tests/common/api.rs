//! The world of an API test: two tenants with one account each, the router of `strata api`,
//! and the calls of the upload routes.

use std::sync::Arc;
use std::time::Duration;

use axum::Router;
use axum::body::{Body, to_bytes};
use axum::http::{HeaderMap, Request, StatusCode};
use axum::routing::get;
use serde_json::{Value, json};
use strata::api::{AppState, router};
use strata::auth::{Auth, keys::KeyCache};
use strata::db::{self, Pool};
use strata::upload::Bounds;
use strata_dev::DevKey;
use tokio_postgres::Client;
use tower::ServiceExt;
use uuid::Uuid;

use super::*;

// ---- The world of a test: two tenants with one account each, and the router.

pub struct Tenant {
    pub id: Uuid,
    pub token: String,
}

pub struct World {
    pub su: Client,
    pub app: Router,
    /// The database of a world in a new database, or empty.
    pub db: String,
    pub a: Tenant,
    pub b: Tenant,
    pub _key: DevKey,
}

/// A JWKS server for `key`.
pub async fn jwks(key: &DevKey) -> String {
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

/// The state of the router with the values of `strata api`, on `pool`. A test changes a field
/// and builds its own router.
pub fn test_state(pool: Pool, bounds: Bounds) -> AppState {
    AppState {
        auth: Auth::new(pool, Arc::new(KeyCache::new())),
        storage: Arc::new(storage()),
        bounds,
        status_retry_after: strata::status::DEFAULT_RETRY_AFTER,
    }
}

pub fn app(pool: Pool, bounds: Bounds) -> Router {
    router(test_state(pool, bounds))
}

/// Two tenants in the database of `su`, and the router on `pool`.
pub async fn world_in(su: Client, pool: Pool, bounds: Bounds) -> World {
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

pub async fn world() -> World {
    world_in(connect(&superuser()).await, api_pool(), Bounds::default()).await
}

/// A world in a new database, for a test that counts the rows of a global table.
pub async fn world_in_new_database(bounds: Bounds) -> World {
    let db = new_database(&connect(&superuser()).await).await;
    let pool = db::pool(config_in("strata_api", &db), 4, db::POOL_WAIT).unwrap();
    let mut w = world_in(connect(&config_in("strata", &db)).await, pool, bounds).await;
    w.db = db;
    w
}

pub struct Answer {
    pub status: StatusCode,
    pub headers: HeaderMap,
    pub body: Value,
}

impl World {
    pub async fn call(&self, req: Request<Body>) -> Answer {
        let resp = self.app.clone().oneshot(req).await.unwrap();
        let (parts, body) = resp.into_parts();
        let bytes = to_bytes(body, 1 << 20).await.unwrap();
        Answer {
            status: parts.status,
            headers: parts.headers,
            body: serde_json::from_slice(&bytes).unwrap_or(Value::Null),
        }
    }

    /// A request of `method` with the token of `t`.
    pub async fn send(
        &self,
        t: &Tenant,
        method: &str,
        path: &str,
        content_type: Option<&str>,
        body: Vec<u8>,
    ) -> Answer {
        let mut req = Request::builder()
            .method(method)
            .uri(path)
            .header("authorization", format!("Bearer {}", t.token));
        if let Some(ct) = content_type {
            req = req.header("content-type", ct);
        }
        self.call(req.body(Body::from(body)).unwrap()).await
    }

    pub async fn post(
        &self,
        t: &Tenant,
        path: &str,
        content_type: Option<&str>,
        body: Vec<u8>,
    ) -> Answer {
        self.send(t, "POST", path, content_type, body).await
    }

    pub async fn get(&self, t: &Tenant, path: &str) -> Answer {
        self.send(t, "GET", path, None, Vec::new()).await
    }

    /// A `QUERY` with a JSON body.
    pub async fn query(&self, t: &Tenant, path: &str, body: &Value) -> Answer {
        let bytes = body.to_string().into_bytes();
        self.send(t, "QUERY", path, Some("application/json"), bytes)
            .await
    }

    pub async fn start(&self, t: &Tenant, document: Option<Uuid>, body: Value) -> Answer {
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

    pub async fn finish(&self, t: &Tenant, document: Uuid, revision: i64) -> Answer {
        self.post(
            t,
            &format!("/v1/documents/{document}/revisions/{revision}/finish"),
            None,
            vec![],
        )
        .await
    }

    /// A start that the API accepts: the document and the revision.
    pub async fn started(
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
    pub async fn uploaded(&self, t: &Tenant, size: usize) -> (Uuid, i64) {
        let (d, r, url) = self.started(t, None, size).await;
        assert_eq!(put(&url, &vec![0u8; size], size, Duration::ZERO).await, 200);
        (d, r)
    }

    pub async fn state(&self, d: Uuid, r: i64) -> (String, Option<String>) {
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

    pub async fn jobs(&self, d: Uuid, r: i64) -> i64 {
        self.su
            .query_one(
                "select count(*) from jobs where document_id = $1 and revision = $2",
                &[&d, &(r as i32)],
            )
            .await
            .unwrap()
            .get(0)
    }

    pub async fn slot_exists(&self, d: Uuid, r: i64) -> bool {
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
    pub async fn rows(&self, t: &Tenant) -> (i64, i64) {
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

pub fn body(size: usize) -> Value {
    json!({"file_name": "report.pdf", "content_type": "application/pdf", "size": size})
}

pub fn location(d: Uuid, r: i64) -> String {
    format!("/v1/documents/{d}/revisions/{r}")
}

pub fn assert_accepted(a: &Answer, d: Uuid, r: i64) {
    assert_eq!(a.status, StatusCode::ACCEPTED, "{}", a.body);
    assert_eq!(a.headers["location"], location(d, r).as_str());
}
