//! The key cache of each issuer: "Signing keys" in tech design 0021.

use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use http_body_util::{BodyExt, Empty, Limited};
use hyper::body::Bytes;
use hyper_rustls::HttpsConnector;
use hyper_util::client::legacy::{Client, connect::HttpConnector};
use hyper_util::rt::TokioExecutor;
use jsonwebtoken::jwk::Jwk;
use jsonwebtoken::{Algorithm, DecodingKey};
use serde_json::Value;
use uuid::Uuid;

/// A fetch of the keys of each issuer again after this time.
pub const REFRESH: Duration = Duration::from_secs(600);
/// At most one more fetch in this time for a `kid` that the cache does not have.
pub const UNKNOWN_KID_RETRY: Duration = Duration::from_secs(60);
const FETCH_TIMEOUT: Duration = Duration::from_secs(5);
const MAX_JWKS_BYTES: usize = 64 * 1024;

type Http = Client<HttpsConnector<HttpConnector>, Empty<Bytes>>;

/// The keys of the issuers, in the memory of one API process.
pub struct KeyCache {
    http: Http,
    refresh: Duration,
    retry: Duration,
    issuers: Mutex<HashMap<Uuid, Arc<tokio::sync::Mutex<Entry>>>>,
}

#[derive(Default)]
struct Entry {
    url: String,
    algorithm: Option<Algorithm>,
    keys: HashMap<String, DecodingKey>,
    tried: Option<Instant>,
}

impl KeyCache {
    /// The cache with the times of 0021.
    pub fn new() -> Self {
        Self::with_times(REFRESH, UNKNOWN_KID_RETRY)
    }

    /// The cache with other times, for the tests.
    pub fn with_times(refresh: Duration, retry: Duration) -> Self {
        let https = hyper_rustls::HttpsConnectorBuilder::new()
            .with_native_roots()
            .expect("the system has root certificates")
            .https_or_http()
            .enable_http1()
            .build();
        Self {
            http: Client::builder(TokioExecutor::new()).build(https),
            refresh,
            retry,
            issuers: Mutex::default(),
        }
    }

    /// The key `kid` of the issuer, or `None`. It can fetch the JWKS, as 0021 says.
    pub async fn key(
        &self,
        issuer_id: Uuid,
        url: &str,
        alg: Algorithm,
        kid: &str,
    ) -> Option<DecodingKey> {
        let entry = self
            .issuers
            .lock()
            .unwrap()
            .entry(issuer_id)
            .or_default()
            .clone();
        let mut e = entry.lock().await;
        if e.url != url || e.algorithm != Some(alg) {
            // The operator changed the issuer: the old keys do not apply.
            *e = Entry {
                url: url.to_owned(),
                algorithm: Some(alg),
                ..Entry::default()
            };
        }
        let since = e.tried.map(|t| t.elapsed());
        let stale = since.is_none_or(|s| s >= self.refresh);
        let retry = !e.keys.contains_key(kid) && since.is_none_or(|s| s >= self.retry);
        if stale || retry {
            e.tried = Some(Instant::now());
            match self.fetch(url).await {
                Ok(body) => e.keys = select_keys(&body, alg),
                // A failed fetch keeps the last keys.
                Err(err) => tracing::warn!(%issuer_id, error = %err, "jwks fetch failed"),
            }
        }
        e.keys.get(kid).cloned()
    }

    async fn fetch(&self, url: &str) -> Result<Bytes, String> {
        let uri = jwks_uri(url).ok_or("not an HTTPS URL or a loopback HTTP URL")?;
        let get = async {
            let resp = self.http.get(uri).await.map_err(|e| e.to_string())?;
            // No redirect: each status other than 200 fails.
            if resp.status() != hyper::StatusCode::OK {
                return Err(format!("status {}", resp.status()));
            }
            let body = Limited::new(resp.into_body(), MAX_JWKS_BYTES)
                .collect()
                .await;
            Ok(body.map_err(|e| e.to_string())?.to_bytes())
        };
        tokio::time::timeout(FETCH_TIMEOUT, get)
            .await
            .map_err(|_| "timeout".to_owned())?
    }
}

impl Default for KeyCache {
    fn default() -> Self {
        Self::new()
    }
}

/// The URI of a JWKS: HTTPS, or HTTP only to a loopback address.
pub fn jwks_uri(url: &str) -> Option<hyper::Uri> {
    let uri: hyper::Uri = url.parse().ok()?;
    let host = uri.host()?.trim_start_matches('[').trim_end_matches(']');
    let loopback = host == "localhost" || host.parse::<IpAddr>().is_ok_and(|ip| ip.is_loopback());
    match uri.scheme_str()? {
        "https" => Some(uri),
        "http" if loopback => Some(uri),
        _ => None,
    }
}

/// The keys of a JWKS that agree with the algorithm of the issuer, by `kid`.
pub fn select_keys(jwks: &[u8], alg: Algorithm) -> HashMap<String, DecodingKey> {
    let Ok(Value::Object(set)) = serde_json::from_slice::<Value>(jwks) else {
        return HashMap::new();
    };
    let Some(Value::Array(keys)) = set.get("keys") else {
        return HashMap::new();
    };
    let (kty, crv) = match alg {
        Algorithm::RS256 => ("RSA", None),
        Algorithm::ES256 => ("EC", Some("P-256")),
        Algorithm::EdDSA => ("OKP", Some("Ed25519")),
        _ => return HashMap::new(),
    };
    let alg_name = format!("{alg:?}");
    keys.iter()
        .filter(|k| {
            k.get("kty").and_then(Value::as_str) == Some(kty)
                && crv.is_none_or(|c| k.get("crv").and_then(Value::as_str) == Some(c))
                && k.get("alg").is_none_or(|a| a.as_str() == Some(&alg_name))
                && k.get("use").is_none_or(|u| u.as_str() == Some("sig"))
        })
        .filter_map(|k| {
            let kid = k.get("kid")?.as_str()?.to_owned();
            let jwk: Jwk = serde_json::from_value(k.clone()).ok()?;
            Some((kid, DecodingKey::from_jwk(&jwk).ok()?))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn okp(kid: &str, extra: Value) -> Value {
        let mut k = json!({"kty": "OKP", "crv": "Ed25519", "kid": kid,
                           "x": "11qYAYKxCrfVS_7TyWQHOg7hcvPapiMlrwIaaPcHURo"});
        k.as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        k
    }

    #[test]
    fn keeps_only_keys_of_the_algorithm() {
        let jwks = json!({"keys": [
            okp("a", json!({})),
            okp("b", json!({"alg": "EdDSA", "use": "sig"})),
            okp("c", json!({"alg": "ES256"})),
            okp("d", json!({"use": "enc"})),
            {"kty": "RSA", "kid": "e", "n": "AQAB", "e": "AQAB"},
            okp("", json!({"kid": 7})),
        ]});
        let keys = select_keys(jwks.to_string().as_bytes(), Algorithm::EdDSA);
        let mut kids: Vec<_> = keys.keys().cloned().collect();
        kids.sort();
        assert_eq!(kids, ["a", "b"]);
        assert!(select_keys(jwks.to_string().as_bytes(), Algorithm::HS256).is_empty());
        assert!(select_keys(b"not json", Algorithm::EdDSA).is_empty());
        assert!(select_keys(b"{\"keys\": 1}", Algorithm::EdDSA).is_empty());
    }

    #[test]
    fn https_or_loopback_http() {
        assert!(jwks_uri("https://p1.example/jwks.json").is_some());
        assert!(jwks_uri("http://127.0.0.1:8090/jwks.json").is_some());
        assert!(jwks_uri("http://[::1]:8090/jwks.json").is_some());
        assert!(jwks_uri("http://localhost/jwks.json").is_some());
        assert!(jwks_uri("http://p1.example/jwks.json").is_none());
        assert!(jwks_uri("http://10.0.0.1/jwks.json").is_none());
        assert!(jwks_uri("ftp://p1.example/jwks.json").is_none());
        assert!(jwks_uri("not a url").is_none());
    }
}
