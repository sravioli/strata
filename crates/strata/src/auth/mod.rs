//! The verification of a token: steps 1 to 10 of tech design 0021.
//!
//! A failure of steps 1 to 9 gives `401` with `WWW-Authenticate: Bearer`. A tenant or an
//! account that is not active gives `403`. Each body is empty, so it shows no step (owner,
//! 2026-11-22). The end of the wait for a connection gives `503` with `Retry-After: 1`.

pub mod claims;
pub mod keys;

use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use axum::extract::{FromRef, FromRequestParts};
use axum::http::{HeaderValue, StatusCode, header, request::Parts};
use axum::response::{IntoResponse, Response};
use jsonwebtoken::{Algorithm, Validation};
use serde_json::{Map, Value};
use uuid::Uuid;

use crate::db::{self, Identity, IdentityTx, Pool};
use keys::KeyCache;

/// The largest token, in bytes (step 1).
pub const MAX_TOKEN_BYTES: usize = 8 * 1024;

/// What the authentication of a request needs. Each API process has one. The pool is private:
/// a route gets a connection only through `Caller`, so each query of a route follows a decision
/// or a scope (TC-4-08).
///
/// ```compile_fail
/// // A route cannot read the pool of the state.
/// fn route(s: strata::api::AppState) {
///     let _ = s.auth.pool;
/// }
/// ```
#[derive(Clone)]
pub struct Auth {
    pool: Pool,
    keys: Arc<KeyCache>,
}

impl Auth {
    pub fn new(pool: Pool, keys: Arc<KeyCache>) -> Self {
        Auth { pool, keys }
    }
}

/// The reason that a request stops before its route runs.
#[derive(Debug, PartialEq, Eq)]
pub enum Reject {
    Unauthorized,
    Forbidden,
    Unavailable,
    /// A start of an upload at a bound (0049): `503` with `Retry-After: 60`.
    Bound,
    Internal,
}

impl IntoResponse for Reject {
    fn into_response(self) -> Response {
        let (status, name, value) = match self {
            Reject::Unauthorized => (StatusCode::UNAUTHORIZED, header::WWW_AUTHENTICATE, "Bearer"),
            Reject::Forbidden => return StatusCode::FORBIDDEN.into_response(),
            Reject::Unavailable => (StatusCode::SERVICE_UNAVAILABLE, header::RETRY_AFTER, "1"),
            Reject::Bound => (
                StatusCode::SERVICE_UNAVAILABLE,
                header::RETRY_AFTER,
                crate::upload::BOUND_RETRY_AFTER,
            ),
            Reject::Internal => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
        };
        (status, [(name, HeaderValue::from_static(value))]).into_response()
    }
}

impl From<db::Error> for Reject {
    fn from(e: db::Error) -> Self {
        if e.is_pool_timeout() {
            tracing::warn!("no connection before the end of the wait");
            Reject::Unavailable
        } else {
            tracing::error!(error = %e, "database error");
            Reject::Internal
        }
    }
}

/// The caller of a request: the identity transaction after steps 1 to 10. A route decides with
/// `authorize` or `authorized_scope`, and opens its tenant transaction with `into_tenant`.
pub struct Caller(pub IdentityTx);

impl<S> FromRequestParts<S> for Caller
where
    Auth: FromRef<S>,
    S: Send + Sync,
{
    type Rejection = Reject;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Reject> {
        let auth = Auth::from_ref(state);
        let token = bearer(parts).ok_or(Reject::Unauthorized)?;
        authenticate(&auth, token).await.map(Caller)
    }
}

fn bearer(parts: &Parts) -> Option<&str> {
    let value = parts.headers.get(header::AUTHORIZATION)?.as_bytes();
    // Step 1, before the parse of the token.
    if value.len() > MAX_TOKEN_BYTES + "Bearer ".len() {
        return None;
    }
    std::str::from_utf8(value).ok()?.strip_prefix("Bearer ")
}

struct IssuerRow {
    issuer_id: Uuid,
    issuer: String,
    jwks_url: String,
    audience: String,
    algorithm: Algorithm,
    tenant_claim: String,
    account_claim: String,
    max_lifetime_s: i64,
}

const READ_ISSUER: &str = "select issuer_id, issuer, jwks_url, audience, algorithm, tenant_claim, \
     account_claim, max_lifetime_s from issuers where issuer = $1 and status = 'active'";

/// Steps 1 to 10 of 0021 for one token.
pub async fn authenticate(auth: &Auth, token: &str) -> Result<IdentityTx, Reject> {
    use Reject::Unauthorized;
    if token.len() > MAX_TOKEN_BYTES {
        return Err(Unauthorized);
    }
    // Step 2: `iss` and `kid` are not verified yet. They only select the issuer and the key.
    let header = jsonwebtoken::decode_header(token).map_err(|_| Unauthorized)?;
    let kid = header.kid.ok_or(Unauthorized)?;
    let unverified: Map<String, Value> =
        jsonwebtoken::dangerous::insecure_decode_claims(token).map_err(|_| Unauthorized)?;
    let iss = unverified
        .get("iss")
        .and_then(Value::as_str)
        .ok_or(Unauthorized)?;

    // Step 3: one statement, with no open transaction. The connection goes back at once.
    let row = {
        let conn = auth.pool.get().await.map_err(db::Error::from)?;
        let st = conn
            .prepare_cached(READ_ISSUER)
            .await
            .map_err(db::Error::from)?;
        conn.query_opt(&st, &[&iss])
            .await
            .map_err(db::Error::from)?
    };
    let row = row.ok_or(Unauthorized)?;
    let algorithm: &str = row.get(4);
    let issuer = IssuerRow {
        issuer_id: row.get(0),
        issuer: row.get(1),
        jwks_url: row.get(2),
        audience: row.get(3),
        algorithm: algorithm.parse().map_err(|_| Reject::Internal)?,
        tenant_claim: row.get(5),
        account_claim: row.get(6),
        max_lifetime_s: i64::from(row.get::<_, i32>(7)),
    };

    // Steps 4 to 6 hold no connection: a fetch of keys can wait.
    let key = auth
        .keys
        .key(issuer.issuer_id, &issuer.jwks_url, issuer.algorithm, &kid)
        .await
        .ok_or(Unauthorized)?;
    // Step 5: only the algorithm of the issuer. `none` and HS* never pass.
    let mut v = Validation::new(issuer.algorithm);
    v.required_spec_claims.clear();
    v.validate_exp = false;
    v.validate_nbf = false;
    v.validate_aud = false;
    let verified =
        jsonwebtoken::decode::<Map<String, Value>>(token, &key, &v).map_err(|_| Unauthorized)?;
    let rules = claims::Rules {
        issuer: &issuer.issuer,
        audience: &issuer.audience,
        tenant_claim: &issuer.tenant_claim,
        account_claim: &issuer.account_claim,
        max_lifetime_s: issuer.max_lifetime_s,
    };
    let claimed = claims::check(&verified.claims, &rules, now()).ok_or(Unauthorized)?;
    if let Some(act) = &claimed.act_sub {
        tracing::info!(act_sub = %act, "request for an actor");
    }

    // Steps 7 to 10 in the identity transaction.
    match IdentityTx::open(
        &auth.pool,
        issuer.issuer_id,
        &claimed.tenant_value,
        &claimed.subject,
    )
    .await?
    {
        Identity::Active(tx) => Ok(*tx),
        Identity::NoBinding => Err(Unauthorized),
        Identity::NotActive => Err(Reject::Forbidden),
    }
}

fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64)
}
