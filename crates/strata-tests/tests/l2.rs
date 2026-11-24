//! L2 tests: the walking skeleton of `make dev` over HTTP (docs/agents/testing.md). `make l2`
//! starts `scripts/dev.sh --background`, runs these tests, and stops the processes. The tests get
//! their tokens from the key file of `strata-dev`. The tenant value `acme` is tenant A, and
//! `globex` is tenant B. A test fails when the API does not answer.

use std::collections::BTreeSet;
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

const API: &str = "127.0.0.1:8080";
const A: &str = "acme";
const B: &str = "globex";
/// The time from a finish to a final status, at most.
const FINAL_WITHIN: Duration = Duration::from_secs(60);

/// A token of the account `sub` of the tenant value `org`, from the key file of `strata-dev`.
fn token(org: &str, sub: &str) -> String {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../.strata-dev/ed25519.der");
    let key = strata_dev::DevKey::load_or_create(std::path::Path::new(path))
        .expect("the key file of strata-dev: run make l2");
    key.token(org, sub, 600)
}

/// A new account of `org`: each run gets new subjects, so the data of earlier runs stays apart.
fn account(org: &str, name: &str) -> String {
    token(org, &format!("{name}-{}", word()))
}

/// A word that no other document contains: letters only, so the parser of `simple` keeps it.
fn word() -> String {
    let mut b = [0u8; 10];
    getrandom::fill(&mut b).unwrap();
    b.iter().map(|x| (b'a' + x % 26) as char).collect()
}

/// A PDF of one page with `text` (Helvetica, so `pdftotext` reads it).
fn pdf(text: &str) -> Vec<u8> {
    let stream = format!("BT /F1 24 Tf 72 700 Td ({text}) Tj ET");
    let objects = [
        "<</Type/Catalog/Pages 2 0 R>>".to_owned(),
        "<</Type/Pages/Kids[3 0 R]/Count 1>>".to_owned(),
        "<</Type/Page/Parent 2 0 R/MediaBox[0 0 612 792]/Resources<</Font<</F1 4 0 R>>>>/Contents 5 0 R>>"
            .to_owned(),
        "<</Type/Font/Subtype/Type1/BaseFont/Helvetica>>".to_owned(),
        format!("<</Length {}>>\nstream\n{stream}\nendstream", stream.len()),
    ];
    let mut out = b"%PDF-1.4\n".to_vec();
    let mut offsets = Vec::new();
    for (i, o) in objects.iter().enumerate() {
        offsets.push(out.len());
        out.extend(format!("{} 0 obj\n{o}\nendobj\n", i + 1).bytes());
    }
    let xref = out.len();
    out.extend(format!("xref\n0 {}\n0000000000 65535 f \n", objects.len() + 1).bytes());
    for o in offsets {
        out.extend(format!("{o:010} 00000 n \n").bytes());
    }
    out.extend(
        format!(
            "trailer\n<</Size {}/Root 1 0 R>>\nstartxref\n{xref}\n%%EOF\n",
            objects.len() + 1
        )
        .bytes(),
    );
    out
}

struct Answer {
    status: u16,
    headers: Vec<(String, String)>,
    body: Value,
}

impl Answer {
    fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }
}

/// One HTTP/1.1 request to `host` with `Connection: close`.
async fn http(
    host: &str,
    method: &str,
    target: &str,
    headers: &[(&str, &str)],
    body: &[u8],
) -> Answer {
    let mut s = tokio::net::TcpStream::connect(host)
        .await
        .unwrap_or_else(|e| panic!("{host} does not answer ({e}): run make l2"));
    let mut head = format!(
        "{method} {target} HTTP/1.1\r\nHost: {host}\r\nContent-Length: {}\r\nConnection: close\r\n",
        body.len()
    );
    for (k, v) in headers {
        head.push_str(&format!("{k}: {v}\r\n"));
    }
    head.push_str("\r\n");
    s.write_all(head.as_bytes()).await.unwrap();
    s.write_all(body).await.unwrap();
    let mut resp = Vec::new();
    s.read_to_end(&mut resp).await.unwrap();
    let text = String::from_utf8_lossy(&resp);
    let (head, body) = text.split_once("\r\n\r\n").expect("an HTTP response");
    let mut lines = head.split("\r\n");
    let status = lines
        .next()
        .unwrap()
        .split(' ')
        .nth(1)
        .unwrap()
        .parse()
        .unwrap();
    let headers = lines
        .filter_map(|l| l.split_once(':'))
        .map(|(k, v)| (k.trim().to_owned(), v.trim().to_owned()))
        .collect();
    Answer {
        status,
        headers,
        body: serde_json::from_str(body).unwrap_or(Value::Null),
    }
}

async fn api(token: &str, method: &str, path: &str, body: Option<&Value>) -> Answer {
    let auth = format!("Bearer {token}");
    let mut headers = vec![("Authorization", auth.as_str())];
    let bytes = body.map(|b| b.to_string().into_bytes()).unwrap_or_default();
    if body.is_some() {
        headers.push(("Content-Type", "application/json"));
    }
    http(API, method, path, &headers, &bytes).await
}

/// An upload of a PDF with `text`, to the finish. Gives the document, the revision and the
/// `Location` of the finish.
async fn upload(token: &str, text: &str) -> (String, i64, String) {
    let file = pdf(text);
    let start =
        json!({"file_name": "l2.pdf", "content_type": "application/pdf", "size": file.len()});
    let a = api(token, "POST", "/v1/documents", Some(&start)).await;
    assert_eq!(a.status, 201, "{}", a.body);
    let document = a.body["document_id"].as_str().unwrap().to_owned();
    let revision = a.body["revision"].as_i64().unwrap();
    let url: url::Url = a.body["upload_url"].as_str().unwrap().parse().unwrap();
    let host = format!(
        "{}:{}",
        url.host_str().unwrap(),
        url.port_or_known_default().unwrap()
    );
    let target = format!("{}?{}", url.path(), url.query().unwrap_or_default());
    assert_eq!(http(&host, "PUT", &target, &[], &file).await.status, 200);
    let path = format!("/v1/documents/{document}/revisions/{revision}/finish");
    let f = api(token, "POST", &path, None).await;
    assert_eq!(f.status, 202, "{}", f.body);
    let location = f.header("Location").expect("a Location").to_owned();
    (document, revision, location)
}

/// The status at `location` when it is final. The poll waits each `Retry-After`.
async fn final_status(token: &str, location: &str) -> Value {
    let end = Instant::now() + FINAL_WITHIN;
    loop {
        let a = api(token, "GET", location, None).await;
        assert!(
            a.status == 200 || a.status == 429,
            "{} {}",
            a.status,
            a.body
        );
        if a.status == 200 && a.header("Retry-After").is_none() {
            return a.body;
        }
        assert!(
            Instant::now() < end,
            "no final status in {FINAL_WITHIN:?}: {}",
            a.body
        );
        let wait: u64 = a.header("Retry-After").unwrap().parse().unwrap();
        tokio::time::sleep(Duration::from_secs(wait)).await;
    }
}

/// The documents of a search for `q`.
async fn search(token: &str, q: &str) -> BTreeSet<String> {
    let a = api(token, "QUERY", "/v1/search", Some(&json!({"q": q}))).await;
    assert_eq!(a.status, 200, "{}", a.body);
    // Only the list of results: no count or other field can show data of a different tenant.
    assert_eq!(a.body.as_object().unwrap().len(), 1, "{}", a.body);
    a.body["results"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["document_id"].as_str().unwrap().to_owned())
        .collect()
}

/// A document of `token` with the text `text`, ready.
async fn ready(token: &str, text: &str) -> String {
    let (document, _, location) = upload(token, text).await;
    assert_eq!(final_status(token, &location).await["state"], "ready");
    document
}

/// TC-49-01: from upload to search locally.
#[tokio::test]
async fn tc_49_01_from_upload_to_search_locally() {
    let (a, b) = (account(A, "anna"), account(B, "boris"));
    let w = word();
    let (document, revision, _) = upload(&a, &format!("report {w}")).await;
    let path = format!("/v1/documents/{document}/revisions/{revision}");
    assert_eq!(final_status(&a, &path).await["state"], "ready");
    assert_eq!(search(&a, &w).await, BTreeSet::from([document]));
    assert!(search(&b, &w).await.is_empty());
}

/// TC-47-11: the `Location` of the finish until the final status.
#[tokio::test]
async fn tc_47_11_the_location_until_the_final_status() {
    let a = account(A, "anna");
    let w = word();
    let (document, revision, location) = upload(&a, &w).await;
    assert_eq!(
        location,
        format!("/v1/documents/{document}/revisions/{revision}")
    );
    let status = final_status(&a, &location).await;
    assert_eq!(status["state"], "ready", "{status}");
    assert_eq!(search(&a, &w).await, BTreeSet::from([document]));
}

/// TC-3-10: a search for the word of a different tenant.
#[tokio::test]
async fn tc_3_10_a_search_for_the_word_of_a_different_tenant() {
    let (a, b) = (account(A, "anna"), account(B, "boris"));
    let (wa, wb) = (word(), word());
    let da = ready(&a, &wa).await;
    let db = ready(&b, &wb).await;
    assert!(search(&a, &wb).await.is_empty());
    assert!(search(&b, &wa).await.is_empty());
    // Each tenant finds its own word, so the empty results come from the scope.
    assert_eq!(search(&a, &wa).await, BTreeSet::from([da]));
    assert_eq!(search(&b, &wb).await, BTreeSet::from([db]));
}

/// TC-3-19: each endpoint with the identifiers of B. Each answer is the same as the answer for
/// identifiers that do not exist, so no answer shows data of B.
#[tokio::test]
async fn tc_3_19_each_endpoint_with_the_identifiers_of_b() {
    let (a, b) = (account(A, "anna"), account(B, "boris"));
    let wb = word();
    let (db, rb, location) = upload(&b, &wb).await;
    assert_eq!(final_status(&b, &location).await["state"], "ready");
    let none = "01a20000-0000-7000-8000-000000000000";
    for (d, r) in [(db.as_str(), rb), (none, 1)] {
        let start = json!({"file_name": "x.pdf", "content_type": "application/pdf", "size": 10});
        let calls = [
            ("POST", format!("/v1/documents/{d}/revisions"), Some(start)),
            (
                "POST",
                format!("/v1/documents/{d}/revisions/{r}/finish"),
                None,
            ),
            ("GET", format!("/v1/documents/{d}/revisions/{r}"), None),
        ];
        for (method, path, body) in calls {
            let ans = api(&a, method, &path, body.as_ref()).await;
            assert_eq!(ans.status, 404, "{method} {path}: {}", ans.body);
            assert_eq!(ans.body, Value::Null, "{method} {path}");
        }
        let batch = json!({"revisions": [{"document_id": d, "revision": r}]});
        let ans = api(&a, "QUERY", "/v1/revisions", Some(&batch)).await;
        assert_eq!(ans.status, 200, "{}", ans.body);
        assert_eq!(ans.body, json!({"revisions": []}));
    }
    assert!(search(&a, &wb).await.is_empty());
    // The status of B for its own revision is there: the 404 of A comes from the scope.
    let ans = api(&b, "GET", &location, None).await;
    assert_eq!(ans.status, 200, "{}", ans.body);
}

/// TC-4-09: two accounts of one tenant and one account of a different tenant.
#[tokio::test]
async fn tc_4_09_two_accounts_of_one_tenant_and_one_of_a_different_tenant() {
    let (anna, bruno, carl) = (account(A, "anna"), account(A, "bruno"), account(B, "carl"));
    let w = word();
    let d1 = ready(&anna, &w).await;
    let d2 = ready(&bruno, &w).await;
    let d3 = ready(&carl, &w).await;
    let a_docs = BTreeSet::from([d1, d2]);
    assert_eq!(search(&anna, &w).await, a_docs);
    assert_eq!(search(&bruno, &w).await, a_docs);
    assert_eq!(search(&carl, &w).await, BTreeSet::from([d3]));
}
