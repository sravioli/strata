//! The object storage: the S3 API with signed URLs (tech design 0006, ADR 0005). `rusty-s3`
//! signs each request (owner, 2026-11-22), and the HTTP client of the API sends the requests of
//! the API. A client gets only a signed PUT URL for the key of one revision.

use std::time::Duration;

use http_body_util::Empty;
use hyper::body::Bytes;
use hyper::{Method, Request, StatusCode, header};
use hyper_rustls::HttpsConnector;
use hyper_util::client::legacy::{Client, connect::HttpConnector};
use hyper_util::rt::TokioExecutor;
use rusty_s3::{Bucket, Credentials, S3Action, UrlStyle};
use url::Url;
use uuid::Uuid;

/// The expiry of an upload URL (0006). It limits the start of a transfer, not its length.
pub const PUT_EXPIRY: Duration = Duration::from_secs(15 * 60);
/// The expiry of the signed URL of a request of the API.
const API_EXPIRY: Duration = Duration::from_secs(60);
/// A request of the API to the storage stops after this time. The finish holds a row lock while
/// it waits.
const TIMEOUT: Duration = Duration::from_secs(10);

pub const ENDPOINT: &str = "AWS_ENDPOINT";
pub const ACCESS_KEY: &str = "AWS_ACCESS_KEY_ID";
pub const SECRET_KEY: &str = "AWS_SECRET_ACCESS_KEY";
pub const REGION: &str = "AWS_DEFAULT_REGION";
pub const BUCKET: &str = "AWS_BUCKET_NAME";
pub const ALLOW_HTTP: &str = "AWS_ALLOW_HTTP";

/// The key of the original file of a revision (0006).
pub fn key(tenant: Uuid, document: Uuid, revision: i32) -> String {
    format!("tenants/{tenant}/documents/{document}/revisions/{revision}/original")
}

#[derive(Debug, thiserror::Error)]
#[error("storage: {0}")]
pub struct Error(String);

type Http = Client<HttpsConnector<HttpConnector>, Empty<Bytes>>;

/// The storage client of one API process.
pub struct Storage {
    bucket: Bucket,
    credentials: Credentials,
    http: Http,
}

impl Storage {
    /// The client from the six variables of the spec document-upload, and no other source.
    pub fn from_env() -> Result<Self, String> {
        Self::from_vars(|name| std::env::var(name).ok())
    }

    /// The client from the values that `var` gives for the six variables. Each error names its
    /// variable.
    pub fn from_vars(var: impl Fn(&str) -> Option<String>) -> Result<Self, String> {
        let need = |name: &str| {
            var(name)
                .filter(|v| !v.is_empty())
                .ok_or_else(|| format!("set {name} for the object storage"))
        };
        let endpoint: Url = need(ENDPOINT)?
            .parse()
            .map_err(|e| format!("{ENDPOINT}: {e}"))?;
        // An http endpoint sends the credentials in plain text: only by choice, for Garage on a laptop.
        if endpoint.scheme() == "http" && var(ALLOW_HTTP).as_deref() != Some("true") {
            return Err(format!(
                "{ENDPOINT} is http: set {ALLOW_HTTP}=true to permit it"
            ));
        }
        let credentials = Credentials::new(need(ACCESS_KEY)?, need(SECRET_KEY)?);
        let bucket = Bucket::new(endpoint, UrlStyle::Path, need(BUCKET)?, need(REGION)?)
            .map_err(|e| format!("{ENDPOINT}: {e}"))?;
        let https = hyper_rustls::HttpsConnectorBuilder::new()
            .with_native_roots()
            .map_err(|e| format!("root certificates: {e}"))?
            .https_or_http()
            .enable_http1()
            .build();
        Ok(Self {
            bucket,
            credentials,
            http: Client::builder(TokioExecutor::new()).build(https),
        })
    }

    /// A URL that permits one PUT of `key`, until `expiry` from now.
    pub fn sign_put(&self, key: &str, expiry: Duration) -> Url {
        self.bucket
            .put_object(Some(&self.credentials), key)
            .sign(expiry)
    }

    /// The size of the object in bytes, or `None` when no object has the key.
    pub async fn size(&self, key: &str) -> Result<Option<u64>, Error> {
        let url = self
            .bucket
            .head_object(Some(&self.credentials), key)
            .sign(API_EXPIRY);
        let resp = self.send(Method::HEAD, url).await?;
        match resp.status() {
            StatusCode::OK => resp
                .headers()
                .get(header::CONTENT_LENGTH)
                .and_then(|v| v.to_str().ok()?.parse().ok())
                .map(Some)
                .ok_or_else(|| Error("HEAD gave no content-length".into())),
            StatusCode::NOT_FOUND => Ok(None),
            s => Err(Error(format!("HEAD gave {s}"))),
        }
    }

    /// Removes the object. A key with no object is not an error.
    pub async fn remove(&self, key: &str) -> Result<(), Error> {
        let url = self
            .bucket
            .delete_object(Some(&self.credentials), key)
            .sign(API_EXPIRY);
        let resp = self.send(Method::DELETE, url).await?;
        match resp.status() {
            s if s.is_success() || s == StatusCode::NOT_FOUND => Ok(()),
            s => Err(Error(format!("DELETE gave {s}"))),
        }
    }

    async fn send(
        &self,
        method: Method,
        url: Url,
    ) -> Result<hyper::Response<hyper::body::Incoming>, Error> {
        let req = Request::builder()
            .method(method)
            .uri(url.as_str())
            .body(Empty::new())
            .map_err(|e| Error(e.to_string()))?;
        tokio::time::timeout(TIMEOUT, self.http.request(req))
            .await
            .map_err(|_| Error("timeout".into()))?
            .map_err(|e| Error(e.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vars(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> {
        let pairs: Vec<(String, String)> = pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        move |name| {
            pairs
                .iter()
                .find(|(k, _)| k == name)
                .map(|(_, v)| v.clone())
        }
    }

    const ALL: &[(&str, &str)] = &[
        (ENDPOINT, "http://localhost:3900"),
        (ACCESS_KEY, "key"),
        (SECRET_KEY, "secret"),
        (REGION, "garage"),
        (BUCKET, "strata"),
        (ALLOW_HTTP, "true"),
    ];

    fn error(pairs: &[(&str, &str)]) -> String {
        Storage::from_vars(vars(pairs)).err().expect("an error")
    }

    #[test]
    fn the_key_of_a_revision() {
        let (t, d) = (Uuid::from_u128(1), Uuid::from_u128(2));
        assert_eq!(
            key(t, d, 3),
            format!("tenants/{t}/documents/{d}/revisions/3/original")
        );
    }

    #[test]
    fn each_missing_variable_is_named() {
        for missing in [ENDPOINT, ACCESS_KEY, SECRET_KEY, REGION, BUCKET] {
            let some: Vec<_> = ALL.iter().copied().filter(|(k, _)| *k != missing).collect();
            assert!(error(&some).contains(missing), "{missing}");
            let empty: Vec<_> = ALL
                .iter()
                .map(|&(k, v)| (k, if k == missing { "" } else { v }))
                .collect();
            assert!(error(&empty).contains(missing), "{missing} empty");
        }
    }

    #[test]
    fn http_needs_allow_http() {
        let no_allow: Vec<_> = ALL
            .iter()
            .copied()
            .filter(|(k, _)| *k != ALLOW_HTTP)
            .collect();
        assert!(error(&no_allow).contains(ALLOW_HTTP));
        let https: Vec<_> = no_allow
            .iter()
            .map(|&(k, v)| {
                (
                    k,
                    if k == ENDPOINT {
                        "https://s3.example"
                    } else {
                        v
                    },
                )
            })
            .collect();
        assert!(Storage::from_vars(vars(&https)).is_ok());
        assert!(Storage::from_vars(vars(ALL)).is_ok());
    }

    #[test]
    fn a_put_url_names_one_key_and_its_expiry() {
        let s = Storage::from_vars(vars(ALL)).unwrap();
        let url = s.sign_put("a/b", PUT_EXPIRY);
        assert_eq!(url.path(), "/strata/a/b");
        let q: Vec<(String, String)> = url.query_pairs().into_owned().collect();
        assert!(q.contains(&("X-Amz-Expires".into(), "900".into())), "{q:?}");
        assert!(q.iter().any(|(k, _)| k == "X-Amz-Signature"));
    }
}
