//! The status routes of tech design 0047: the public status of a revision, one revision or a
//! batch, with `Retry-After` for a status that is not final.

use axum::Json;
use axum::extract::{FromRequestParts, Path, Request, State};
use axum::http::{HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use serde_json::{Value, json};
use strata_core::authorization::{Action, authorized_scope};
use strata_core::revision::{Revision, RevisionState};
use uuid::Uuid;

use crate::api::AppState;
use crate::auth::{Caller, Reject};
use crate::db::{IdentityTx, TenantTx, status as q};

/// The status of one revision as the database holds it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Status {
    pub document_id: Uuid,
    pub revision: i32,
    pub r: Revision,
    /// `status_at` of the revision, in whole seconds since 1970.
    pub updated_at: i64,
}

/// The public status of each internal state (the map of 0047). The job does not count, so the
/// revision of a `Parked` job is `processing` (TC-47-02).
pub fn public_status(state: RevisionState) -> &'static str {
    match state {
        RevisionState::AwaitingUpload => "uploading",
        RevisionState::Uploaded => "processing",
        RevisionState::Ready => "ready",
        RevisionState::Failed => "failed",
        RevisionState::Superseded => "superseded",
    }
}

/// `ready`, `failed` and `superseded` are final (0047).
pub fn is_final(state: RevisionState) -> bool {
    !matches!(
        state,
        RevisionState::AwaitingUpload | RevisionState::Uploaded
    )
}

/// The JSON of one status: `error_class` for `failed` only, and `no_text` for `ready` only.
pub fn status_json(s: &Status) -> Value {
    let updated_at = jiff::Timestamp::from_second(s.updated_at)
        .unwrap_or(jiff::Timestamp::UNIX_EPOCH)
        .to_string();
    let mut v = json!({
        "document_id": s.document_id.to_string(),
        "revision": s.revision,
        "state": public_status(s.r.state),
        "updated_at": updated_at,
    });
    match s.r.state {
        RevisionState::Failed => {
            if let Some(class) = s.r.error_class {
                v["error_class"] = json!(format!("{class:?}"));
            }
        }
        RevisionState::Ready => v["no_text"] = json!(s.r.no_text),
        _ => {}
    }
    v
}

/// Each response of the two status routes: a status changes, so no cache keeps it (0047).
fn no_store(mut r: Response) -> Response {
    r.headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    r
}

/// The answer for a revision that does not exist, of a different tenant, or with an identifier
/// that is not valid: one `404` for each (0047, [#3](https://github.com/sravioli/strata/issues/3)).
fn not_found() -> Response {
    StatusCode::NOT_FOUND.into_response()
}

/// The tenant transaction of a status read: the scope of `ReadDocument` (0004).
async fn read_tx(id: IdentityTx) -> Result<TenantTx, Reject> {
    let scope = authorized_scope(id.principal(), Action::ReadDocument);
    id.into_tenant(scope).await?.ok_or(Reject::Forbidden)
}

/// The response of statuses: `Retry-After` when one of them is not final.
fn with_retry_after(mut r: Response, any_open: bool, retry_after: u64) -> Response {
    if any_open {
        r.headers_mut()
            .insert(header::RETRY_AFTER, HeaderValue::from(retry_after));
    }
    r
}

/// `GET /v1/documents/{document_id}/revisions/{revision}`: the status of one revision.
pub async fn one(
    State(s): State<AppState>,
    Path((document_id, revision)): Path<(String, String)>,
    req: Request,
) -> Response {
    no_store(match read(&s, &document_id, &revision, req).await {
        Ok(r) => r,
        Err(reject) => reject.into_response(),
    })
}

async fn read(
    s: &AppState,
    document_id: &str,
    revision: &str,
    req: Request,
) -> Result<Response, Reject> {
    let (mut parts, _) = req.into_parts();
    let Caller(id) = Caller::from_request_parts(&mut parts, s).await?;
    let (Ok(document_id), Ok(revision)) = (document_id.parse::<Uuid>(), revision.parse::<i32>())
    else {
        return Ok(not_found());
    };
    let tx = read_tx(id).await?;
    Ok(match q::read_one(tx, document_id, revision).await? {
        Some(st) => with_retry_after(
            (StatusCode::OK, Json(status_json(&st))).into_response(),
            !is_final(st.r.state),
            s.status_retry_after,
        ),
        None => not_found(),
    })
}

/// The setting of the `Retry-After` of a status that is not final, in seconds.
pub const RETRY_AFTER_VAR: &str = "STRATA_STATUS_RETRY_AFTER";
/// The default of 0047.
pub const DEFAULT_RETRY_AFTER: u64 = 2;

/// The value of `STRATA_STATUS_RETRY_AFTER`: whole seconds from 1 to 3,600, and the default when
/// the variable is absent. Each other value is an error that names the variable.
pub fn retry_after(value: Option<&str>) -> Result<u64, String> {
    let Some(v) = value else {
        return Ok(DEFAULT_RETRY_AFTER);
    };
    match v.parse::<u64>() {
        Ok(n @ 1..=3600) => Ok(n),
        _ => Err(format!(
            "{RETRY_AFTER_VAR}: {v:?} is not whole seconds from 1 to 3600"
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use strata_core::revision::FailureClass;

    fn status(state: RevisionState, error_class: Option<FailureClass>, no_text: bool) -> Value {
        status_json(&Status {
            document_id: Uuid::nil(),
            revision: 3,
            r: Revision {
                state,
                retry_mark: false,
                error_class,
                no_text,
            },
            updated_at: 1_795_000_502,
        })
    }

    #[test]
    fn each_internal_state_has_its_public_status() {
        use RevisionState::*;
        let map = [
            (AwaitingUpload, "uploading", false),
            (Uploaded, "processing", false),
            (Ready, "ready", true),
            (Failed, "failed", true),
            (Superseded, "superseded", true),
        ];
        for (state, public, last) in map {
            assert_eq!(public_status(state), public);
            assert_eq!(is_final(state), last, "{public}");
        }
    }

    #[test]
    fn the_optional_fields() {
        let failed = status(
            RevisionState::Failed,
            Some(FailureClass::FileTooLarge),
            false,
        );
        assert_eq!(failed["error_class"], "FileTooLarge");
        assert!(failed.get("no_text").is_none());
        let ready = status(RevisionState::Ready, None, true);
        assert_eq!(ready["no_text"], true);
        assert!(ready.get("error_class").is_none());
        for state in [
            RevisionState::AwaitingUpload,
            RevisionState::Uploaded,
            RevisionState::Superseded,
        ] {
            let v = status(state, None, false);
            assert!(v.get("no_text").is_none() && v.get("error_class").is_none());
        }
        assert_eq!(ready["revision"], 3);
        assert_eq!(ready["document_id"], Uuid::nil().to_string());
        // RFC 3339, UTC, whole seconds.
        assert_eq!(ready["updated_at"], "2026-11-18T11:15:02Z");
    }

    #[test]
    fn the_setting_and_its_limits() {
        assert_eq!(retry_after(None), Ok(2));
        assert_eq!(retry_after(Some("1")), Ok(1));
        assert_eq!(retry_after(Some("3600")), Ok(3600));
        for bad in ["0", "3601", "x", "-1", ""] {
            assert!(
                retry_after(Some(bad))
                    .unwrap_err()
                    .contains(RETRY_AFTER_VAR)
            );
        }
    }
}
