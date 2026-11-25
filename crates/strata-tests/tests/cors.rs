//! L1 tests of the spec cross-origin-access (tech design 0065, ADR 0013): the CORS layer of the
//! router of `strata api`, with and without `STRATA_CORS_ORIGINS`.

mod common;

use std::path::PathBuf;
use std::process::Command;

use axum::body::Body;
use axum::http::{HeaderMap, Request, StatusCode};
use common::api::*;
use common::*;
use strata::api::router;
use strata::upload::Bounds;

const SITE: &str = "http://localhost:4321";

/// A world whose router allows `origins`.
async fn world_with(origins: &[&str]) -> World {
    let mut w = world().await;
    let mut state = test_state(api_pool(), Bounds::default());
    state.cors_origins = origins.iter().map(|o| o.parse().unwrap()).collect();
    w.app = router(state);
    w
}

/// A preflight of `QUERY /v1/search` from `origin`, with the two headers of the site.
fn preflight(origin: &str) -> Request<Body> {
    Request::builder()
        .method("OPTIONS")
        .uri("/v1/search")
        .header("origin", origin)
        .header("access-control-request-method", "QUERY")
        .header(
            "access-control-request-headers",
            "authorization,content-type",
        )
        .body(Body::empty())
        .unwrap()
}

/// A `GET` of the status of a revision in `AwaitingUpload` of tenant A, from `origin`.
async fn status_from(w: &World, origin: Option<&str>) -> Answer {
    let (d, _) = document(&w.su, w.a.id, "AwaitingUpload").await;
    let mut req = Request::builder()
        .uri(format!("/v1/documents/{d}/revisions/1"))
        .header("authorization", format!("Bearer {}", w.a.token));
    if let Some(o) = origin {
        req = req.header("origin", o);
    }
    w.call(req.body(Body::empty()).unwrap()).await
}

fn cors_headers(h: &HeaderMap) -> Vec<String> {
    h.keys()
        .map(|k| k.as_str().to_owned())
        .filter(|k| k.starts_with("access-control-"))
        .collect()
}

/// TC-65-01
#[tokio::test]
async fn tc_65_01_no_setting_no_cors_headers() {
    let w = world().await;
    let pre = w.call(preflight(SITE)).await;
    assert_eq!(cors_headers(&pre.headers), Vec::<String>::new());
    let with = status_from(&w, Some(SITE)).await;
    let without = status_from(&w, None).await;
    assert_eq!(cors_headers(&with.headers), Vec::<String>::new());
    assert_eq!(with.status, StatusCode::OK);
    assert_eq!(with.status, without.status);
    assert_eq!(with.body["state"], without.body["state"]);
    let names = |h: &HeaderMap| {
        let mut v: Vec<_> = h.keys().map(|k| k.as_str().to_owned()).collect();
        v.sort();
        v
    };
    assert_eq!(names(&with.headers), names(&without.headers));
}

/// TC-65-02
#[tokio::test]
async fn tc_65_02_a_preflight_of_the_allowed_origin() {
    let w = world_with(&[SITE]).await;
    let a = w.call(preflight(SITE)).await;
    assert_eq!(a.status, StatusCode::OK);
    let h = &a.headers;
    assert_eq!(h["access-control-allow-origin"], SITE);
    let list = |name: &str| -> Vec<String> {
        h[name]
            .to_str()
            .unwrap()
            .split(',')
            .map(|s| s.trim().to_ascii_lowercase())
            .collect()
    };
    assert_eq!(
        list("access-control-allow-methods"),
        ["get", "post", "query"]
    );
    assert_eq!(
        list("access-control-allow-headers"),
        ["authorization", "content-type"]
    );
    assert_eq!(h["access-control-max-age"], "600");
    assert!(h.get("access-control-allow-credentials").is_none());
    let vary: Vec<_> = h
        .get_all("vary")
        .iter()
        .map(|v| v.to_str().unwrap())
        .collect();
    assert!(
        vary.iter()
            .any(|v| v.to_ascii_lowercase().contains("origin")),
        "{vary:?}"
    );
}

/// TC-65-03
#[tokio::test]
async fn tc_65_03_origins_not_on_the_list() {
    let w = world_with(&[SITE]).await;
    for origin in ["http://localhost:4322", "null"] {
        let pre = w.call(preflight(origin)).await;
        assert!(
            pre.headers.get("access-control-allow-origin").is_none(),
            "{origin}"
        );
        let get = status_from(&w, Some(origin)).await;
        assert_eq!(get.status, StatusCode::OK);
        assert!(
            get.headers.get("access-control-allow-origin").is_none(),
            "{origin}"
        );
    }
}

/// TC-65-04
#[tokio::test]
async fn tc_65_04_the_api_exposes_retry_after() {
    let w = world_with(&[SITE]).await;
    let a = status_from(&w, Some(SITE)).await;
    assert_eq!(a.status, StatusCode::OK);
    assert_eq!(a.headers["retry-after"], "2");
    assert_eq!(a.headers["access-control-allow-origin"], SITE);
    let exposed = a.headers["access-control-expose-headers"]
        .to_str()
        .unwrap()
        .to_ascii_lowercase();
    assert!(
        exposed.split(',').any(|s| s.trim() == "retry-after"),
        "{exposed}"
    );
}

/// The `strata` binary of the build of this test, as in `telemetry.rs`.
fn strata_bin() -> PathBuf {
    let exe = std::env::current_exe().unwrap();
    exe.ancestors()
        .map(|a| a.join("strata"))
        .find(|p| p.is_file())
        .unwrap_or_else(|| panic!("build strata first ({})", exe.display()))
}

/// TC-65-05
#[test]
fn tc_65_05_bad_origins_stop_the_start() {
    for bad in [
        "*",
        "http://localhost:4321/",
        "http://localhost:4321/app",
        "localhost:4321",
        "ftp://localhost",
    ] {
        let mut cmd = Command::new(strata_bin());
        cmd.args(["api", "--listen", "127.0.0.1:0"])
            .env_clear()
            .env(
                strata::DATABASE_URL,
                "postgres://strata_api:strata_api@localhost:6432/strata",
            )
            .env(strata::cors::ORIGINS_VAR, bad);
        // The coverage build writes its profile where the variable says.
        if let Ok(p) = std::env::var("LLVM_PROFILE_FILE") {
            cmd.env("LLVM_PROFILE_FILE", p);
        }
        let out = cmd.output().unwrap();
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(!out.status.success(), "{bad}: {stderr}");
        assert!(
            stderr.contains(strata::cors::ORIGINS_VAR),
            "{bad}: {stderr}"
        );
    }
}
