//! The development issuer of tech design 0021: one Ed25519 key pair, its JWKS on loopback, and
//! tokens for a tenant value and a subject. The server crate does not depend on it (TC-21-17).

use std::io::Write;
use std::os::unix::fs::OpenOptionsExt;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use jsonwebtoken::jwk::{Jwk, JwkSet};
use jsonwebtoken::{Algorithm, EncodingKey, Header};
use serde_json::{Value, json};

/// The `iss` of each token, and the value that `strata admin issuer register` needs.
pub const ISSUER: &str = "strata-dev";
pub const AUDIENCE: &str = "strata";
pub const TENANT_CLAIM: &str = "tenant";
pub const KID: &str = "strata-dev-1";
pub const DEFAULT_LIFETIME_S: i64 = 3600;

/// The PKCS#8 v1 prefix of an Ed25519 private key (RFC 8410). The 32-byte seed follows it.
const PKCS8_PREFIX: [u8; 16] = [
    0x30, 0x2e, 0x02, 0x01, 0x00, 0x30, 0x05, 0x06, 0x03, 0x2b, 0x65, 0x70, 0x04, 0x22, 0x04, 0x20,
];

/// One Ed25519 key pair, as PKCS#8 DER.
pub struct DevKey {
    der: Vec<u8>,
}

impl DevKey {
    pub fn generate() -> Self {
        let mut seed = [0u8; 32];
        getrandom::fill(&mut seed).expect("the system gives random bytes");
        Self {
            der: [&PKCS8_PREFIX[..], &seed[..]].concat(),
        }
    }

    /// The key pair in `path`. The first run makes it, in a file that only the user can read.
    pub fn load_or_create(path: &Path) -> std::io::Result<Self> {
        match std::fs::read(path) {
            Ok(der) if der.len() == 48 && der.starts_with(&PKCS8_PREFIX) => Ok(Self { der }),
            Ok(_) => Err(std::io::Error::other(format!(
                "{}: not an Ed25519 key",
                path.display()
            ))),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                if let Some(dir) = path.parent() {
                    std::fs::create_dir_all(dir)?;
                }
                let key = Self::generate();
                let mut f = std::fs::OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .mode(0o600)
                    .open(path)?;
                f.write_all(&key.der)?;
                Ok(key)
            }
            Err(e) => Err(e),
        }
    }

    fn encoding_key(&self) -> EncodingKey {
        EncodingKey::from_ed_der(&self.der)
    }

    /// The JWKS with the public key, as the API fetches it.
    pub fn jwks(&self) -> Value {
        let mut jwk =
            Jwk::from_encoding_key(&self.encoding_key(), Algorithm::EdDSA).expect("an Ed25519 key");
        jwk.common.key_id = Some(KID.to_owned());
        serde_json::to_value(JwkSet { keys: vec![jwk] }).expect("a JWKS is JSON")
    }

    /// A token with these claims, signed with EdDSA and the `kid` of the JWKS.
    pub fn sign(&self, claims: &Value) -> String {
        let mut header = Header::new(Algorithm::EdDSA);
        header.kid = Some(KID.to_owned());
        jsonwebtoken::encode(&header, claims, &self.encoding_key()).expect("an Ed25519 key signs")
    }

    /// A token for a tenant value and a subject, valid for `lifetime_s` from now.
    pub fn token(&self, tenant_value: &str, subject: &str, lifetime_s: i64) -> String {
        self.sign(&claims(tenant_value, subject, now(), lifetime_s))
    }
}

/// The claims of a token of the development issuer.
pub fn claims(tenant_value: &str, subject: &str, iat: i64, lifetime_s: i64) -> Value {
    json!({"iss": ISSUER, "aud": AUDIENCE, "sub": subject, TENANT_CLAIM: tenant_value,
           "iat": iat, "exp": iat + lifetime_s})
}

pub fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64)
}

/// The operator commands that register the development issuer with a JWKS URL.
pub fn register_commands(jwks_url: &str) -> String {
    format!(
        "strata admin issuer register --issuer {ISSUER} --jwks-url {jwks_url} --audience {AUDIENCE} \
         --algorithm EdDSA --tenant-claim {TENANT_CLAIM}\n\
         strata admin binding add --issuer {ISSUER} --tenant-value <value> --tenant <name>\n"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use jsonwebtoken::{DecodingKey, Validation};

    #[test]
    fn token_verifies_with_the_jwks() {
        let key = DevKey::generate();
        let set: JwkSet = serde_json::from_value(key.jwks()).unwrap();
        assert_eq!(set.keys[0].common.key_id.as_deref(), Some(KID));
        let dk = DecodingKey::from_jwk(&set.keys[0]).unwrap();
        let mut v = Validation::new(Algorithm::EdDSA);
        v.set_audience(&[AUDIENCE]);
        v.set_issuer(&[ISSUER]);
        let t = jsonwebtoken::decode::<Value>(key.token("acme", "alice", 60), &dk, &v).unwrap();
        assert_eq!(t.claims["sub"], "alice");
        assert_eq!(t.claims[TENANT_CLAIM], "acme");
        assert_eq!(
            t.claims["exp"].as_i64().unwrap() - t.claims["iat"].as_i64().unwrap(),
            60
        );
    }

    #[test]
    fn same_key_on_the_second_run() {
        let dir = std::env::temp_dir().join(format!("strata-dev-test-{}", std::process::id()));
        let path = dir.join("key.der");
        let first = DevKey::load_or_create(&path).unwrap().jwks();
        assert_eq!(DevKey::load_or_create(&path).unwrap().jwks(), first);
        let mode = std::os::unix::fs::PermissionsExt::mode(
            &std::fs::metadata(&path).unwrap().permissions(),
        );
        assert_eq!(mode & 0o777, 0o600);
        std::fs::write(&path, b"junk").unwrap();
        assert!(DevKey::load_or_create(&path).is_err());
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn commands_name_the_url() {
        assert!(
            register_commands("http://127.0.0.1:8090/jwks.json").contains(
                "issuer register --issuer strata-dev --jwks-url http://127.0.0.1:8090/jwks.json"
            )
        );
    }
}
