//! The CORS layer of `strata api` (tech design 0065, ADR 0013): an exact list of origins in
//! `STRATA_CORS_ORIGINS`. With no origins, the router has no layer.

use std::time::Duration;

use axum::http::{HeaderValue, Method, header};
use tower_http::cors::{AllowOrigin, CorsLayer};

/// The setting of the origins of browser clients.
pub const ORIGINS_VAR: &str = "STRATA_CORS_ORIGINS";

/// The origins of `STRATA_CORS_ORIGINS`, separated by commas. An absent or empty value gives no
/// origins. Each other item must be an exact origin, or the value is an error that names the
/// variable.
pub fn origins(value: Option<&str>) -> Result<Vec<HeaderValue>, String> {
    let v = value.unwrap_or("").trim();
    if v.is_empty() {
        return Ok(Vec::new());
    }
    v.split(',')
        .map(str::trim)
        .map(|item| match is_origin(item) {
            true => Ok(HeaderValue::from_str(item).expect("an origin is ASCII")),
            false => Err(format!(
                "{ORIGINS_VAR}: {item:?} is not an origin (http or https, a host and an optional \
                 port, with no path)"
            )),
        })
        .collect()
}

/// `http` or `https`, a lowercase host, and an optional port: the form of a browser `Origin`.
fn is_origin(item: &str) -> bool {
    let Some(rest) = item
        .strip_prefix("http://")
        .or_else(|| item.strip_prefix("https://"))
    else {
        return false;
    };
    // An IPv6 host is in brackets, and its colons are not the port.
    let port = match rest.strip_prefix('[') {
        Some(r) => match r.split_once(']') {
            Some((h, p))
                if !h.is_empty() && h.chars().all(|c| c.is_ascii_hexdigit() || c == ':') =>
            {
                p
            }
            _ => return false,
        },
        None => {
            let (h, p) = rest.split_at(rest.find(':').unwrap_or(rest.len()));
            let host_char =
                |c: char| c.is_ascii_lowercase() || c.is_ascii_digit() || "-.".contains(c);
            if h.is_empty() || !h.chars().all(host_char) {
                return false;
            }
            p
        }
    };
    match port.strip_prefix(':') {
        None => port.is_empty(),
        Some(p) => !p.starts_with('0') && p.parse::<u16>().is_ok_and(|n| n > 0),
    }
}

/// The layer for `origins`: the methods, the headers and the max age of 0065. It sends no
/// `Access-Control-Allow-Credentials`, because the token is in a header and not in a cookie.
pub fn layer(origins: Vec<HeaderValue>) -> CorsLayer {
    CorsLayer::new()
        .allow_origin(AllowOrigin::list(origins))
        .allow_methods([
            Method::GET,
            Method::POST,
            Method::from_bytes(b"QUERY").expect("a method name"),
        ])
        .allow_headers([header::AUTHORIZATION, header::CONTENT_TYPE])
        .expose_headers([header::RETRY_AFTER])
        .max_age(Duration::from_secs(600))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ok(v: &str) -> Vec<String> {
        origins(Some(v))
            .unwrap()
            .iter()
            .map(|h| h.to_str().unwrap().to_owned())
            .collect()
    }

    #[test]
    fn no_value_gives_no_origins() {
        assert!(origins(None).unwrap().is_empty());
        assert!(origins(Some("")).unwrap().is_empty());
        assert!(origins(Some("  ")).unwrap().is_empty());
    }

    #[test]
    fn exact_origins_with_spaces_around_the_commas() {
        assert_eq!(
            ok(" http://localhost:4321 , https://demo.example.com,http://[::1]:8080"),
            [
                "http://localhost:4321",
                "https://demo.example.com",
                "http://[::1]:8080"
            ]
        );
        assert_eq!(ok("http://127.0.0.1:65535"), ["http://127.0.0.1:65535"]);
    }

    #[test]
    fn each_kind_of_bad_item() {
        for bad in [
            "*",
            "null",
            "http://*.example.com",
            "localhost:4321",
            "ftp://localhost",
            "http://localhost:4321/",
            "http://localhost:4321/app",
            "http://localhost:4321?x=1",
            "http://localhost:4321#x",
            "http://user@localhost:4321",
            "http://",
            "http://:4321",
            "http://localhost:",
            "http://localhost:0",
            "http://localhost:65536",
            "http://localhost:04321",
            "http://LOCALHOST:4321",
            "HTTP://localhost:4321",
            "http://[]:80",
            "http://[::1",
            "http://localhost:4321,",
            "http://localhost:4321,,http://localhost:4322",
        ] {
            let e = origins(Some(bad)).expect_err(bad);
            assert!(e.contains(ORIGINS_VAR), "{bad}: {e}");
        }
    }
}
