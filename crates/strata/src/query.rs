//! The routes of the method `QUERY` (RFC 10008): the search of 0049 and the batch status of 0047.
//! axum 0.8 has no method filter for `QUERY`, so each such route takes each method, and its
//! handler checks the method first (0049).

use axum::body::to_bytes;
use axum::extract::Request;
use axum::handler::Handler;
use axum::http::request::Parts;
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::{MethodRouter, any};
use serde_json::Value;

use crate::api::AppState;
use crate::upload::{MAX_BODY_BYTES, json_body};

/// The route of a `QUERY` path. The name tells the route check of TC-3-18 the method.
pub fn query<H, T>(handler: H) -> MethodRouter<AppState>
where
    H: Handler<T, AppState>,
    T: 'static,
{
    any(handler)
}

/// Why a `QUERY` request stops before its handler reads it.
#[derive(Debug, PartialEq, Eq)]
pub enum Refused {
    /// A method other than `QUERY`: `405` with `Allow: QUERY`.
    Method,
    /// A body rule of 0047.
    Body(StatusCode),
}

impl IntoResponse for Refused {
    fn into_response(self) -> Response {
        match self {
            Refused::Method => {
                (StatusCode::METHOD_NOT_ALLOWED, [(header::ALLOW, "QUERY")]).into_response()
            }
            Refused::Body(status) => status.into_response(),
        }
    }
}

/// The method, then the body rules of 0047, then the JSON. The check of the method comes before
/// the read of the token and of the body. The body comes before the token, so no request holds a
/// connection while its body arrives.
pub async fn read(req: Request) -> Result<(Parts, Value), Refused> {
    if req.method().as_str() != "QUERY" {
        return Err(Refused::Method);
    }
    let (parts, body) = req.into_parts();
    // A body of more than the limit stops at the limit.
    let body = to_bytes(body, MAX_BODY_BYTES + 1)
        .await
        .map_err(|_| Refused::Body(StatusCode::PAYLOAD_TOO_LARGE))?;
    let v = json_body(parts.headers.get(header::CONTENT_TYPE), &body).map_err(Refused::Body)?;
    Ok((parts, v))
}
