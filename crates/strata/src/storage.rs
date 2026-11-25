//! The object storage: the S3 API with signed URLs (tech design 0006, ADR 0005). `rusty-s3`
//! signs each request (owner, 2026-11-22), and the HTTP client of the API sends the requests of
//! the API. A client gets only a signed PUT URL for the key of one revision.

use std::time::Duration;

use http_body_util::{BodyExt, Empty};
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
/// The read of an object by the worker stops after this time, with its body.
// shortcut: one fixed time for 100 MB; make it a setting if a storage is slower than 1 MB/s.
const READ_TIMEOUT: Duration = Duration::from_secs(120);
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

/// The longest `ETag` that a revision keeps (the check of V6).
pub const MAX_ETAG: usize = 1024;

/// The answer to a HEAD of an object.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Head {
    pub size: u64,
    /// The `ETag` header as the storage sends it, with its quotes.
    pub etag: String,
}

/// The answer to a read of an object.
#[derive(Debug, PartialEq, Eq)]
pub enum Object {
    Missing,
    /// The object is not the object of the `ETag` of the read.
    Changed,
    Bytes(Vec<u8>),
}

/// A fixed text or a status code. It never holds the text of an error of the HTTP client, which
/// can hold the signed URL (0133).
#[derive(Debug, thiserror::Error)]
#[error("storage: {0}")]
pub struct Error(String);

type Http = Client<HttpsConnector<HttpConnector>, Empty<Bytes>>;

/// The storage client of one API or worker process.
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
        // An http endpoint sends the credentials in plain text: only by choice, for a local Garage.
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

    /// A URL that permits one PUT of `key` until `expiry` from now, and the time of its end.
    pub fn sign_put(&self, key: &str, expiry: Duration) -> (Url, jiff::Timestamp) {
        // Whole seconds, as `X-Amz-Date` of the signature: the end is then exact.
        let now = jiff::Timestamp::from_second(jiff::Timestamp::now().as_second())
            .expect("the time now is a valid timestamp");
        let url = self
            .bucket
            .put_object(Some(&self.credentials), key)
            .sign_with_time(expiry, &now);
        let end = now
            .checked_add(expiry)
            .expect("an expiry of minutes stays in the range of a timestamp");
        (url, end)
    }

    /// The size and the `ETag` of the object, or `None` when no object has the key.
    pub async fn head(&self, key: &str) -> Result<Option<Head>, Error> {
        let url = self
            .bucket
            .head_object(Some(&self.credentials), key)
            .sign(API_EXPIRY);
        let resp = self.send(Method::HEAD, url, None).await?;
        let text = |name| resp.headers().get(name).and_then(|v| v.to_str().ok());
        match resp.status() {
            StatusCode::OK => {
                let size = text(header::CONTENT_LENGTH).and_then(|v| v.parse().ok());
                let etag = text(header::ETAG).filter(|v| (1..=MAX_ETAG).contains(&v.len()));
                match (size, etag) {
                    (Some(size), Some(etag)) => Ok(Some(Head {
                        size,
                        etag: etag.to_owned(),
                    })),
                    _ => Err(Error("HEAD gave no content-length or no etag".into())),
                }
            }
            StatusCode::NOT_FOUND => Ok(None),
            s => Err(Error(format!("HEAD gave {s}"))),
        }
    }

    /// At most `max` + 1 bytes of the object, so that the caller can see an object above `max`.
    /// With `if_match`, only the object of that `ETag`: an object that changed after the finish is
    /// `Object::Changed`. The worker reads with its own credentials (0006).
    pub async fn read(&self, key: &str, max: u64, if_match: Option<&str>) -> Result<Object, Error> {
        let url = self
            .bucket
            .get_object(Some(&self.credentials), key)
            .sign(API_EXPIRY);
        let limit = usize::try_from(max).unwrap_or(usize::MAX).saturating_add(1);
        let read = async {
            let resp = self.send(Method::GET, url, if_match).await?;
            match resp.status() {
                StatusCode::OK => {}
                StatusCode::NOT_FOUND => return Ok(Object::Missing),
                StatusCode::PRECONDITION_FAILED => return Ok(Object::Changed),
                s => return Err(Error(format!("GET gave {s}"))),
            }
            let mut body = resp.into_body();
            let mut bytes = Vec::new();
            while bytes.len() < limit {
                let Some(frame) = body.frame().await else {
                    break;
                };
                let frame = frame.map_err(|_| Error("the read of the body failed".into()))?;
                if let Ok(data) = frame.into_data() {
                    bytes.extend_from_slice(&data[..data.len().min(limit - bytes.len())]);
                }
            }
            Ok(Object::Bytes(bytes))
        };
        tokio::time::timeout(READ_TIMEOUT, read)
            .await
            .map_err(|_| Error("timeout".into()))?
    }

    /// Removes the object. A key with no object is not an error.
    pub async fn remove(&self, key: &str) -> Result<(), Error> {
        let url = self
            .bucket
            .delete_object(Some(&self.credentials), key)
            .sign(API_EXPIRY);
        let resp = self.send(Method::DELETE, url, None).await?;
        match resp.status() {
            s if s.is_success() || s == StatusCode::NOT_FOUND => Ok(()),
            s => Err(Error(format!("DELETE gave {s}"))),
        }
    }

    async fn send(
        &self,
        method: Method,
        url: Url,
        if_match: Option<&str>,
    ) -> Result<hyper::Response<hyper::body::Incoming>, Error> {
        let mut req = Request::builder().method(method).uri(url.as_str());
        if let Some(etag) = if_match {
            req = req.header(header::IF_MATCH, etag);
        }
        let req = req
            .body(Empty::new())
            .map_err(|_| Error("the request is not valid".into()))?;
        tokio::time::timeout(TIMEOUT, self.http.request(req))
            .await
            .map_err(|_| Error("timeout".into()))?
            .map_err(|e| {
                // The kind of a connection error, with no text.
                let class = if e.is_connect() {
                    "no connection"
                } else {
                    "the send failed"
                };
                Error(class.into())
            })
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
        let (url, end) = s.sign_put("a/b", PUT_EXPIRY);
        let left = end.duration_since(jiff::Timestamp::now());
        assert!(left.as_secs() > 890 && left.as_secs() <= 900, "{left:?}");
        assert_eq!(url.path(), "/strata/a/b");
        let q: Vec<(String, String)> = url.query_pairs().into_owned().collect();
        assert!(q.contains(&("X-Amz-Expires".into(), "900".into())), "{q:?}");
        assert!(q.iter().any(|(k, _)| k == "X-Amz-Signature"));
    }
}
