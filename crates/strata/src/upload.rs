//! The upload routes (tech designs 0006, 0047 and 0049, spec document-upload). Each route gets
//! its tenant transaction from `authorized_scope(principal, UploadDocument)`. A route with a
//! `document_id` cannot know the tenant of the document before the query, so the scope and the
//! policy of 0003 hide each document of a different tenant, and the route answers `404`.

use axum::Json;
use axum::body::to_bytes;
use axum::extract::{FromRequestParts, Path, Request, State};
use axum::http::{HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use serde_json::{Value, json};
use strata_core::authorization::{Action, authorized_scope};
use strata_core::revision::{Event, FailureClass, RevisionState, revision_step};
use uuid::Uuid;

use crate::api::AppState;
use crate::auth::{Caller, Reject};
use crate::db::upload::{self as q, NewRevision, Started};
use crate::db::{IdentityTx, TenantTx};
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
async fn start_tx(s: &AppState, req: Request) -> Result<(StartBody, TenantTx), Stop> {
    let (mut parts, body) = req.into_parts();
    // A body of more than the limit stops at the limit.
    let body = to_bytes(body, MAX_BODY_BYTES + 1)
        .await
        .map_err(|_| Stop::Status(StatusCode::PAYLOAD_TOO_LARGE))?;
    let start =
        check_start(parts.headers.get(header::CONTENT_TYPE), &body).map_err(Stop::Status)?;
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
        return Err(Stop::Reject(Reject::Bound));
    }
    Ok((start, tx))
}

/// Why a start stops before its write: a rule of the body, or a rejection of the request.
enum Stop {
    Status(StatusCode),
    Reject(Reject),
}

impl From<Reject> for Stop {
    fn from(r: Reject) -> Self {
        Stop::Reject(r)
    }
}

impl IntoResponse for Stop {
    fn into_response(self) -> Response {
        match self {
            Stop::Status(s) => s.into_response(),
            Stop::Reject(r) => r.into_response(),
        }
    }
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
pub async fn start_document(State(s): State<AppState>, req: Request) -> Result<Response, Reject> {
    let (start, tx) = match start_tx(&s, req).await {
        Ok(v) => v,
        Err(stop) => return Ok(stop.into_response()),
    };
    let tenant = tx.tenant_id();
    let started = q::start_document(tx, &start.as_new()).await?;
    Ok(created(&s, tenant, started))
}

/// `POST /v1/documents/{document_id}/revisions`: the start of an upload of the next revision.
pub async fn start_revision(
    State(s): State<AppState>,
    Path(document_id): Path<Uuid>,
    req: Request,
) -> Result<Response, Reject> {
    let (start, tx) = match start_tx(&s, req).await {
        Ok(v) => v,
        Err(stop) => return Ok(stop.into_response()),
    };
    let tenant = tx.tenant_id();
    Ok(
        match q::start_revision(tx, document_id, &start.as_new()).await? {
            Some(started) => created(&s, tenant, started),
            None => StatusCode::NOT_FOUND.into_response(),
        },
    )
}

/// `POST /v1/documents/{document_id}/revisions/{revision}/finish`: one answer for each state.
pub async fn finish(
    State(s): State<AppState>,
    Path((document_id, revision)): Path<(Uuid, i32)>,
    Caller(id): Caller,
) -> Result<Response, Reject> {
    let mut tx = upload_tx(id).await?;
    let Some(locked) = q::lock_revision(&mut tx, document_id, revision).await? else {
        tx.rollback().await?;
        return Ok(StatusCode::NOT_FOUND.into_response());
    };
    let r = locked.revision;
    let key = storage::key(tx.tenant_id(), document_id, revision);
    if r.state != RevisionState::AwaitingUpload {
        tx.rollback().await?;
        if r.error_class == Some(FailureClass::FileTooLarge) {
            // Each finish tries the removal again, so a failed removal does not stay.
            remove_too_large(&s, &key).await;
            return Ok(StatusCode::PAYLOAD_TOO_LARGE.into_response());
        }
        return Ok(accepted(document_id, revision));
    }
    let size = match s.storage.size(&key).await {
        Ok(Some(n)) => n,
        Ok(None) => {
            tx.rollback().await?;
            return Ok(StatusCode::CONFLICT.into_response());
        }
        Err(e) => {
            tx.rollback().await?;
            return Err(storage_error(e));
        }
    };
    let too_large = size > MAX_FILE_BYTES as u64;
    let event = if too_large {
        Event::FinishTooLarge
    } else {
        Event::Finish
    };
    let next = revision_step(r, event);
    q::write_finish(tx, document_id, revision, next, locked.slot_id, true).await?;
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
