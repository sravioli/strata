//! The upload routes (tech designs 0006, 0047 and 0049, spec document-upload). Each route gets
//! its tenant transaction from `authorized_scope(principal, UploadDocument)`. A route with a
//! `document_id` cannot know the tenant of the document before the query, so the scope and the
//! policy of 0003 hide each document of a different tenant, and the route answers `404`.

use axum::Json;
use std::time::Duration;

use axum::body::{Body, Bytes, to_bytes};
use axum::extract::{FromRequestParts, Path, Request, State};
use axum::http::request::Parts;
use axum::http::{HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use serde_json::{Value, json};
use strata_core::authorization::{Action, authorized_scope};
use strata_core::revision::{Event, FailureClass, Revision, RevisionState, revision_step};
use uuid::Uuid;

use crate::api::AppState;
use crate::auth::{Caller, Reject};
use crate::db::upload::{self as q, NewRevision, Started};
use crate::db::{IdentityTx, TenantTx};
use crate::query::Refused;
use crate::storage::{self, PUT_EXPIRY};

/// 100 MB (0006).
pub const MAX_FILE_BYTES: i64 = 104_857_600;
/// The largest request body (0047, 0049 "Values").
pub const MAX_BODY_BYTES: usize = 16_384;
/// The longest `file_name` and `content_type`, in characters (owner, 2026-11-22).
pub const MAX_TEXT_CHARS: usize = 1024;
/// The `Retry-After` of a start at a bound, in seconds (owner, 2026-11-22).
pub const BOUND_RETRY_AFTER: &str = "60";

/// The two bounds of a start (0049, "Values"). A parameter of the router, so a test can set a
/// small bound.
#[derive(Clone, Copy, Debug)]
pub struct Bounds {
    pub waiting_jobs: i64,
    pub upload_slots: i64,
}

impl Default for Bounds {
    fn default() -> Self {
        Bounds {
            waiting_jobs: 20_000,
            upload_slots: 5_000,
        }
    }
}

/// The longest wait for a whole request body. A client that sends its body slowly holds a permit
/// of `limits::bound`, so a body that does not arrive in this time gets `408`.
pub const BODY_TIMEOUT: Duration = Duration::from_secs(10);

/// The body of a request, with at most `MAX_BODY_BYTES` + 1 bytes: `413` for a longer body, `408`
/// for a body that does not arrive in `within`.
pub async fn read_body(body: Body, within: Duration) -> Result<Bytes, StatusCode> {
    match tokio::time::timeout(within, to_bytes(body, MAX_BODY_BYTES + 1)).await {
        Err(_) => Err(StatusCode::REQUEST_TIMEOUT),
        // A body of more than the limit stops at the limit.
        Ok(read) => read.map_err(|_| StatusCode::PAYLOAD_TOO_LARGE),
    }
}

/// The start body after its check.
#[derive(Debug, PartialEq, Eq)]
pub struct StartBody {
    pub file_name: String,
    pub content_type: String,
    pub size: i64,
}

/// The body rules of 0047 for each route with a JSON body: no `Content-Type` gives `400`, a
/// media type other than `application/json` gives `415`, a body above 16 KB gives `413`, and a
/// body that is not JSON gives `400`.
pub fn json_body(content_type: Option<&HeaderValue>, body: &[u8]) -> Result<Value, StatusCode> {
    let media = content_type.ok_or(StatusCode::BAD_REQUEST)?;
    let media = media
        .to_str()
        .map_err(|_| StatusCode::UNSUPPORTED_MEDIA_TYPE)?;
    let essence = media.split(';').next().unwrap_or_default().trim();
    if !essence.eq_ignore_ascii_case("application/json") {
        return Err(StatusCode::UNSUPPORTED_MEDIA_TYPE);
    }
    if body.len() > MAX_BODY_BYTES {
        return Err(StatusCode::PAYLOAD_TOO_LARGE);
    }
    serde_json::from_slice(body).map_err(|_| StatusCode::BAD_REQUEST)
}

/// The rules of the start body: the body rules of 0047, then the fields. Each rejection is the
/// status of the spec.
pub fn check_start(
    content_type: Option<&HeaderValue>,
    body: &[u8],
) -> Result<StartBody, StatusCode> {
    let v = json_body(content_type, body)?;
    let text = |name: &str| {
        v.get(name)
            .and_then(Value::as_str)
            .filter(|s| (1..=MAX_TEXT_CHARS).contains(&s.chars().count()))
            .map(str::to_owned)
            .ok_or(StatusCode::BAD_REQUEST)
    };
    let (file_name, content_type) = (text("file_name")?, text("content_type")?);
    let size = v
        .get("size")
        .and_then(Value::as_number)
        .ok_or(StatusCode::BAD_REQUEST)?;
    let size = match (size.as_u64(), size.as_i64()) {
        (Some(n), _) if n > MAX_FILE_BYTES as u64 => return Err(StatusCode::PAYLOAD_TOO_LARGE),
        (Some(n), _) => n as i64,
        // A negative integer or a number with a fraction.
        _ => return Err(StatusCode::BAD_REQUEST),
    };
    Ok(StartBody {
        file_name,
        content_type,
        size,
    })
}

impl StartBody {
    fn as_new(&self) -> NewRevision<'_> {
        NewRevision {
            file_name: &self.file_name,
            content_type: &self.content_type,
            size: self.size,
        }
    }
}

fn location(document_id: Uuid, revision: i32) -> HeaderValue {
    HeaderValue::from_str(&format!("/v1/documents/{document_id}/revisions/{revision}"))
        .expect("a UUID and a number are a valid header value")
}

fn accepted(document_id: Uuid, revision: i32) -> Response {
    (
        StatusCode::ACCEPTED,
        [(header::LOCATION, location(document_id, revision))],
    )
        .into_response()
}

fn storage_error(e: storage::Error) -> Reject {
    tracing::error!(error = %e, "storage error");
    Reject::Unavailable
}

/// The tenant transaction of an upload route.
async fn upload_tx(id: IdentityTx) -> Result<TenantTx, Reject> {
    let scope = authorized_scope(id.principal(), Action::UploadDocument);
    id.into_tenant(scope).await?.ok_or(Reject::Forbidden)
}

/// The body first, so no request holds a connection while its body arrives. Then steps 1 to 10
/// of 0021, and the tenant transaction.
async fn start_tx(s: &AppState, req: Request) -> Result<(StartBody, TenantTx), Refused> {
    let (mut parts, body) = req.into_parts();
    let body = read_body(body, BODY_TIMEOUT).await.map_err(Refused::Body)?;
    let start =
        check_start(parts.headers.get(header::CONTENT_TYPE), &body).map_err(Refused::Body)?;
    let Caller(id) = Caller::from_request_parts(&mut parts, s).await?;
    let mut tx = upload_tx(id).await?;
    let (waiting, slots) = q::bound_counts(&mut tx).await.map_err(Reject::from)?;
    let bound = if waiting >= s.bounds.waiting_jobs {
        Some("waiting_jobs")
    } else if slots >= s.bounds.upload_slots {
        Some("upload_slots")
    } else {
        None
    };
    if let Some(bound) = bound {
        tracing::warn!(bound, "start of an upload rejected at a bound");
        tx.rollback().await.map_err(Reject::from)?;
        return Err(Reject::Bound.into());
    }
    Ok((start, tx))
}

/// The answer to a start after its commit.
fn created(s: &AppState, tenant: Uuid, started: Started) -> Response {
    let key = storage::key(tenant, started.document_id, started.revision);
    let (url, expires_at) = s.storage.sign_put(&key, PUT_EXPIRY);
    let body = json!({
        "document_id": started.document_id.to_string(),
        "revision": started.revision,
        "upload_url": url.as_str(),
        "expires_at": expires_at.to_string(),
    });
    (StatusCode::CREATED, Json(body)).into_response()
}

/// `POST /v1/documents`: the start of an upload of a new document.
pub async fn start_document(State(s): State<AppState>, req: Request) -> Result<Response, Refused> {
    let (start, tx) = start_tx(&s, req).await?;
    let tenant = tx.tenant_id();
    let started = q::start_document(tx, &start.as_new())
        .await
        .map_err(Reject::from)?;
    Ok(created(&s, tenant, started))
}

/// `POST /v1/documents/{document_id}/revisions`: the start of an upload of the next revision.
pub async fn start_revision(
    State(s): State<AppState>,
    Path(document_id): Path<Uuid>,
    req: Request,
) -> Result<Response, Refused> {
    let (start, tx) = start_tx(&s, req).await?;
    let tenant = tx.tenant_id();
    Ok(
        match q::start_revision(tx, document_id, &start.as_new())
            .await
            .map_err(Reject::from)?
        {
            Some(started) => created(&s, tenant, started),
            None => StatusCode::NOT_FOUND.into_response(),
        },
    )
}

/// The steps of 0021, the tenant transaction, and the row lock of the revision. `None` when the
/// tenant has no such revision: then the transaction rolled back.
async fn lock(
    s: &AppState,
    parts: &mut Parts,
    document_id: Uuid,
    revision: i32,
) -> Result<Option<(TenantTx, q::Locked)>, Reject> {
    let Caller(id) = Caller::from_request_parts(parts, s).await?;
    let mut tx = upload_tx(id).await?;
    match q::lock_revision(&mut tx, document_id, revision).await? {
        Some(locked) => Ok(Some((tx, locked))),
        None => {
            tx.rollback().await?;
            Ok(None)
        }
    }
}

/// The answer to a finish of a revision that left `AwaitingUpload`: the same answer each time.
async fn finished(
    s: &AppState,
    r: Revision,
    key: &str,
    document_id: Uuid,
    revision: i32,
) -> Response {
    if r.error_class == Some(FailureClass::FileTooLarge) {
        // Each finish tries the removal again, so a failed removal does not stay.
        remove_too_large(s, key).await;
        return StatusCode::PAYLOAD_TOO_LARGE.into_response();
    }
    accepted(document_id, revision)
}

/// `POST /v1/documents/{document_id}/revisions/{revision}/finish`: one answer for each state.
///
/// The HEAD of the storage runs between two transactions, so a slow storage holds no connection
/// of the pool and no row lock. The second transaction runs the steps of 0021 again, because a
/// tenant transaction comes only from them (TC-4-08), and writes only if no other finish wrote
/// first.
pub async fn finish(
    State(s): State<AppState>,
    Path((document_id, revision)): Path<(Uuid, i32)>,
    mut parts: Parts,
) -> Result<Response, Reject> {
    let Some((tx, locked)) = lock(&s, &mut parts, document_id, revision).await? else {
        return Ok(StatusCode::NOT_FOUND.into_response());
    };
    let tenant = tx.tenant_id();
    tx.rollback().await?;
    let key = storage::key(tenant, document_id, revision);
    if locked.revision.state != RevisionState::AwaitingUpload {
        return Ok(finished(&s, locked.revision, &key, document_id, revision).await);
    }
    let head = match s.storage.head(&key).await {
        Ok(Some(head)) => head,
        Ok(None) => return Ok(StatusCode::CONFLICT.into_response()),
        Err(e) => return Err(storage_error(e)),
    };

    let Some((tx, locked)) = lock(&s, &mut parts, document_id, revision).await? else {
        return Ok(StatusCode::NOT_FOUND.into_response());
    };
    let r = locked.revision;
    if tx.tenant_id() != tenant || r.state != RevisionState::AwaitingUpload {
        // A different tenant cannot happen with the same token. A finish that wrote first.
        tx.rollback().await?;
        return Ok(finished(&s, r, &key, document_id, revision).await);
    }
    let too_large = head.size > MAX_FILE_BYTES as u64;
    let event = if too_large {
        Event::FinishTooLarge
    } else {
        Event::Finish
    };
    let next = revision_step(r, event);
    q::write_finish(
        tx,
        document_id,
        revision,
        next,
        locked.slot_id,
        &head.etag,
        true,
    )
    .await?;
    if too_large {
        remove_too_large(&s, &key).await;
        return Ok(StatusCode::PAYLOAD_TOO_LARGE.into_response());
    }
    Ok(accepted(document_id, revision))
}

/// The removal of an object above the maximum, after the commit. A failure goes to the log, and
/// the next finish of the revision tries again.
async fn remove_too_large(s: &AppState, key: &str) {
    if let Err(e) = s.storage.remove(key).await {
        tracing::warn!(error = %e, "removal of an object above the maximum failed");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn json() -> HeaderValue {
        HeaderValue::from_static("application/json")
    }

    fn body(file_name: &str, content_type: &str, size: &str) -> Vec<u8> {
        format!(
            r#"{{"file_name": {}, "content_type": {}, "size": {size}}}"#,
            Value::from(file_name),
            Value::from(content_type)
        )
        .into_bytes()
    }

    fn check(b: &[u8]) -> Result<StartBody, StatusCode> {
        check_start(Some(&json()), b)
    }

    #[test]
    fn the_size_at_and_past_its_limits() {
        assert_eq!(check(&body("a", "b", "0")).unwrap().size, 0);
        assert_eq!(
            check(&body("a", "b", "104857600")).unwrap().size,
            MAX_FILE_BYTES
        );
        assert_eq!(
            check(&body("a", "b", "104857601")),
            Err(StatusCode::PAYLOAD_TOO_LARGE)
        );
        assert_eq!(
            check(&body("a", "b", "18446744073709551615")),
            Err(StatusCode::PAYLOAD_TOO_LARGE)
        );
        for bad in ["-1", "1.5", "\"10\"", "null"] {
            assert_eq!(
                check(&body("a", "b", bad)),
                Err(StatusCode::BAD_REQUEST),
                "{bad}"
            );
        }
    }

    #[test]
    fn the_texts_at_and_past_their_limits() {
        let at = "é".repeat(MAX_TEXT_CHARS);
        let past = "é".repeat(MAX_TEXT_CHARS + 1);
        assert!(check(&body(&at, &at, "1")).is_ok());
        for (f, c) in [
            ("", "b"),
            ("a", ""),
            (past.as_str(), "b"),
            ("a", past.as_str()),
        ] {
            assert_eq!(check(&body(f, c, "1")), Err(StatusCode::BAD_REQUEST));
        }
        let missing = br#"{"content_type": "b", "size": 1}"#;
        assert_eq!(check(missing), Err(StatusCode::BAD_REQUEST));
        assert_eq!(check(b"[]"), Err(StatusCode::BAD_REQUEST));
        assert_eq!(check(b"{"), Err(StatusCode::BAD_REQUEST));
    }

    /// A body that never sends a byte.
    struct Silent;

    impl hyper::body::Body for Silent {
        type Data = Bytes;
        type Error = std::convert::Infallible;

        fn poll_frame(
            self: std::pin::Pin<&mut Self>,
            _: &mut std::task::Context<'_>,
        ) -> std::task::Poll<Option<Result<hyper::body::Frame<Bytes>, Self::Error>>> {
            std::task::Poll::Pending
        }
    }

    #[tokio::test]
    async fn a_body_that_does_not_arrive_in_time() {
        let wait = Duration::from_millis(50);
        assert_eq!(
            read_body(Body::new(Silent), wait).await,
            Err(StatusCode::REQUEST_TIMEOUT)
        );
        let long = Body::from(vec![b' '; MAX_BODY_BYTES + 2]);
        assert_eq!(
            read_body(long, wait).await,
            Err(StatusCode::PAYLOAD_TOO_LARGE)
        );
        let ok = read_body(Body::from("{}"), wait).await.unwrap();
        assert_eq!(&ok[..], b"{}");
    }

    #[test]
    fn the_rules_of_the_body_itself() {
        let ok = body("a", "b", "1");
        assert_eq!(check_start(None, &ok), Err(StatusCode::BAD_REQUEST));
        let text = HeaderValue::from_static("text/plain");
        assert_eq!(
            check_start(Some(&text), &ok),
            Err(StatusCode::UNSUPPORTED_MEDIA_TYPE)
        );
        let charset = HeaderValue::from_static("Application/JSON; charset=utf-8");
        assert!(check_start(Some(&charset), &ok).is_ok());
        let mut at = body("a", "b", "1");
        at.resize(MAX_BODY_BYTES, b' ');
        assert!(check(&at).is_ok());
        at.push(b' ');
        assert_eq!(check(&at), Err(StatusCode::PAYLOAD_TOO_LARGE));
    }
}
