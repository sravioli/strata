//! TC-3-18: each route of the router of `strata api` has a test with the identifiers of a
//! different tenant (spec tenant-isolation). The check reads each `.route(` call of
//! `strata::api::router`, and each marker of a cross-tenant test: a doc comment that starts with
//! "Cross-tenant:", then the method and the path of the route in backticks. It needs no service.

use std::collections::BTreeSet;
use std::path::Path;

type Route = (String, String);

/// The (method, path) of each `.route(` call in the body of `pub fn router`.
// shortcut: one method filter for each route, as the router has; a chain such as `get(a).post(b)`
// gives only its first method. Parse the chain when a route gets two methods.
fn routes(api: &str) -> BTreeSet<Route> {
    let start = api
        .find("pub fn router(")
        .expect("the router of strata api");
    let body = &api[start..];
    let body = &body[..body.find("\n}\n").expect("the end of the router")];
    body.split(".route(")
        .skip(1)
        .map(|call| {
            let mut parts = call.splitn(3, '"');
            let path = parts.nth(1).expect("a path in quotes").to_owned();
            let rest = parts.next().unwrap_or_default();
            let method: String = rest
                .trim_start_matches([',', ' ', '\n'])
                .chars()
                .take_while(|c| c.is_ascii_alphabetic())
                .collect();
            (method.to_ascii_uppercase(), path)
        })
        .collect()
}

/// The (method, path) of each marker in `text`.
fn markers(text: &str) -> BTreeSet<Route> {
    const MARK: &str = "/// Cross-tenant: `";
    text.match_indices(MARK)
        .filter_map(|(i, _)| {
            let rest = &text[i + MARK.len()..];
            let (method, path) = rest[..rest.find('`')?].split_once(' ')?;
            Some((method.to_owned(), path.to_owned()))
        })
        .collect()
}

fn missing(routes: &BTreeSet<Route>, markers: &BTreeSet<Route>) -> Vec<Route> {
    routes.difference(markers).cloned().collect()
}

fn tests_text() -> String {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests");
    let mut text = String::new();
    for e in std::fs::read_dir(dir).unwrap() {
        let path = e.unwrap().path();
        if path.extension().is_some_and(|x| x == "rs") && !path.ends_with("routes.rs") {
            text.push_str(&std::fs::read_to_string(path).unwrap());
        }
    }
    text
}

fn api_text() -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../strata/src/api.rs");
    std::fs::read_to_string(path).unwrap()
}

/// TC-3-18
#[test]
fn each_route_has_a_test_with_the_identifiers_of_a_different_tenant() {
    let routes = routes(&api_text());
    assert_eq!(routes.len(), 5, "{routes:?}");
    assert_eq!(
        missing(&routes, &markers(&tests_text())),
        Vec::<Route>::new()
    );
}

/// TC-3-18: the check fails when a route has no such test.
#[test]
fn the_check_fails_when_a_route_has_no_test() {
    let api = api_text().replacen(
        ".route(\"/v1/documents\",",
        ".route(\"/v1/other\", get(other))\n        .route(\"/v1/documents\",",
        1,
    );
    let missing = missing(&routes(&api), &markers(&tests_text()));
    assert_eq!(missing, [("GET".to_owned(), "/v1/other".to_owned())]);
}
