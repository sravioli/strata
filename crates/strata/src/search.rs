//! `QUERY /v1/search` (tech designs 0049 and 0145): one hit for each matching location of the
//! caller's tenant, best first, each with a snippet as segments.

use std::time::Duration;

use axum::Json;
use axum::extract::{FromRequestParts, Request, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde_json::{Value, json};
use strata_core::authorization::{Action, authorized_scope};

use crate::api::AppState;
use crate::auth::{Caller, Reject};
use crate::db::search as q;
use crate::query::{self, Refused};

/// The statement timeout of a search (0049). [#73](https://github.com/sravioli/strata/issues/73)
/// removes it before M2 ends.
pub const TIMEOUT: Duration = Duration::from_secs(2);
const MAX_Q_CHARS: usize = 1000;
const MAX_LIMIT: i64 = 100;
const DEFAULT_LIMIT: i64 = 20;

/// The search body `{"q": …, "limit": …}`: `q` of 1 to 1,000 characters, `limit` from 1 to 100
/// and 20 when it is absent. `None` for each other body.
pub fn parse(v: &Value) -> Option<(String, i64)> {
    let body = v.as_object()?;
    if body.keys().any(|k| k != "q" && k != "limit") {
        return None;
    }
    let text = body
        .get("q")?
        .as_str()
        .filter(|s| (1..=MAX_Q_CHARS).contains(&s.chars().count()))?;
    let limit = match body.get("limit") {
        None => DEFAULT_LIMIT,
        Some(l) => l.as_i64().filter(|n| (1..=MAX_LIMIT).contains(n))?,
    };
    Some((text.to_owned(), limit))
}

/// The start and the stop mark of a matched word in the output of `ts_headline` (0145).
const START: char = '\u{2}';
const STOP: char = '\u{3}';

/// The segments of a snippet: `{"t": …}` for text and `{"t": …, "match": true}` for a matched
/// word, with no empty segment and no mark. A start mark with no stop mark makes the rest a
/// match. A stop mark with no start mark is text.
pub fn segments(snippet: &str) -> Vec<Value> {
    let mut out = Vec::new();
    let mut piece = String::new();
    let mut matched = false;
    let mut flush = |piece: &mut String, matched: bool| {
        if !piece.is_empty() {
            let t = std::mem::take(piece);
            out.push(if matched {
                json!({"t": t, "match": true})
            } else {
                json!({ "t": t })
            });
        }
    };
    for c in snippet.chars() {
        match c {
            START | STOP => {
                flush(&mut piece, matched);
                matched = c == START;
            }
            _ => piece.push(c),
        }
    }
    flush(&mut piece, matched);
    out
}

/// `QUERY /v1/search` with the action `Search`.
pub async fn search(State(s): State<AppState>, req: Request) -> Response {
    run(&s, req).await.into_response()
}

async fn run(s: &AppState, req: Request) -> Result<Response, Refused> {
    let (mut parts, v) = query::read(req).await?;
    let Some((text, limit)) = parse(&v) else {
        return Ok(StatusCode::BAD_REQUEST.into_response());
    };
    // The length of `q`, never `q` (0133).
    let span = tracing::Span::current();
    span.record("strata.search.q_length", text.chars().count());
    span.record("strata.search.limit", limit);
    let Caller(id) = Caller::from_request_parts(&mut parts, s).await?;
    let scope = authorized_scope(id.principal(), Action::Search);
    let tx = id
        .into_tenant(scope)
        .await
        .map_err(Reject::from)?
        .ok_or(Reject::Forbidden)?;
    let timeout_ms = u64::try_from(s.search_timeout.as_millis()).unwrap_or(u64::MAX);
    let hits = match q::search(tx, &text, limit, timeout_ms).await {
        Ok(hits) => hits,
        Err(e) if e.is_statement_timeout() => {
            // The log holds no text of the query.
            tracing::warn!(timeout_ms, "a search reached its statement timeout");
            s.metrics
                .rejection(crate::telemetry::Rejection::SearchTimeout);
            return Err(Reject::Unavailable.into());
        }
        Err(e) => return Err(Reject::from(e).into()),
    };
    let results: Vec<Value> = hits
        .iter()
        .map(|h| {
            json!({
                "document_id": h.document_id.to_string(),
                "revision": h.revision,
                "score": h.score,
                "location": {"kind": h.location_kind, "number": h.location_number},
                "text": segments(&h.snippet),
            })
        })
        .collect();
    Ok((StatusCode::OK, Json(json!({ "results": results }))).into_response())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_search_body() {
        assert_eq!(parse(&json!({"q": "quartz"})), Some(("quartz".into(), 20)));
        assert_eq!(parse(&json!({"q": "a", "limit": 1})), Some(("a".into(), 1)));
        assert_eq!(
            parse(&json!({"q": "a", "limit": 100})).map(|p| p.1),
            Some(100)
        );
        let long = "é".repeat(1000);
        assert!(parse(&json!({ "q": long })).is_some());
        for bad in [
            json!({}),
            json!({"q": ""}),
            json!({ "q": "é".repeat(1001) }),
            json!({"q": 1}),
            json!({"q": "a", "limit": 0}),
            json!({"q": "a", "limit": 101}),
            json!({"q": "a", "limit": 2.5}),
            json!({"q": "a", "x": 1}),
            json!(["q"]),
        ] {
            assert_eq!(parse(&bad), None, "{bad}");
        }
    }

    #[test]
    fn the_segments_of_a_snippet() {
        let m = |t: &str| json!({"t": t, "match": true});
        let t = |t: &str| json!({ "t": t });
        assert_eq!(segments("no mark"), [t("no mark")]);
        assert_eq!(segments(""), Vec::<Value>::new());
        assert_eq!(
            segments("alpha \u{2}bravo\u{3} charlie"),
            [t("alpha "), m("bravo"), t(" charlie")]
        );
        assert_eq!(
            segments("\u{2}start\u{3} middle \u{2}end\u{3}"),
            [m("start"), t(" middle "), m("end")]
        );
        assert_eq!(segments("\u{2}a\u{3}\u{2}b\u{3}"), [m("a"), m("b")]);
        assert_eq!(segments("a \u{2}rest"), [t("a "), m("rest")]);
        assert_eq!(segments("a\u{3} b"), [t("a"), t(" b")]);
        assert_eq!(segments("\u{2}\u{3}"), Vec::<Value>::new());
    }
}
