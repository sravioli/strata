//! Step 6 of the verification of a token (tech design 0021): the claims, after the signature.
//! Pure functions, with no clock: the caller gives the current time.

use serde_json::{Map, Value};

/// Tolerance for the clocks of the issuer and of the API, in seconds.
pub const LEEWAY_S: f64 = 60.0;
/// The longest tenant value, subject and `act.sub`, in bytes.
pub const MAX_CLAIM_BYTES: usize = 256;

/// The settings of one issuer row that the claims must agree with.
pub struct Rules<'a> {
    pub issuer: &'a str,
    pub audience: &'a str,
    pub tenant_claim: &'a str,
    pub account_claim: &'a str,
    pub max_lifetime_s: i64,
}

/// The values that a valid token gives.
#[derive(Debug, PartialEq, Eq)]
pub struct Claimed {
    pub tenant_value: String,
    pub subject: String,
    /// `act.sub`, cut to 256 bytes. It goes to the request log only.
    pub act_sub: Option<String>,
}

/// The claims of step 6 of 0021, or `None` when one rule fails. `now` is in Unix seconds.
pub fn check(claims: &Map<String, Value>, r: &Rules, now: i64) -> Option<Claimed> {
    let now = now as f64;
    if claims.get("iss")?.as_str()? != r.issuer || !audience_ok(claims.get("aud")?, r.audience) {
        return None;
    }
    let exp = claims.get("exp")?.as_f64()?;
    let iat = claims.get("iat")?.as_f64()?;
    if now > exp + LEEWAY_S || iat > now + LEEWAY_S || exp - iat > r.max_lifetime_s as f64 {
        return None;
    }
    if let Some(nbf) = claims.get("nbf")
        && now < nbf.as_f64()? - LEEWAY_S
    {
        return None;
    }
    Some(Claimed {
        tenant_value: short_string(claims.get(r.tenant_claim)?)?,
        subject: short_string(claims.get(r.account_claim)?)?,
        act_sub: claims
            .get("act")
            .and_then(|a| a.get("sub"))
            .and_then(Value::as_str)
            .map(|s| cut(s, MAX_CLAIM_BYTES).to_owned()),
    })
}

fn audience_ok(aud: &Value, audience: &str) -> bool {
    match aud {
        Value::String(s) => s == audience,
        Value::Array(a) => a.iter().any(|v| v.as_str() == Some(audience)),
        _ => false,
    }
}

/// One string of 1 to 256 bytes.
fn short_string(v: &Value) -> Option<String> {
    let s = v.as_str()?;
    (!s.is_empty() && s.len() <= MAX_CLAIM_BYTES).then(|| s.to_owned())
}

/// The longest prefix of `s` with at most `max` bytes that ends on a character boundary.
pub fn cut(s: &str, max: usize) -> &str {
    let mut end = s.len().min(max);
    while !s.is_char_boundary(end) {
        end -= 1;
    }
    &s[..end]
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const NOW: i64 = 1_800_000_000;
    const RULES: Rules = Rules {
        issuer: "https://p1.example",
        audience: "strata",
        tenant_claim: "org",
        account_claim: "sub",
        max_lifetime_s: 3600,
    };

    fn valid() -> Map<String, Value> {
        json!({"iss": "https://p1.example", "aud": "strata", "exp": NOW + 600,
               "iat": NOW - 10, "org": "acme-corp", "sub": "u-1"})
        .as_object()
        .unwrap()
        .clone()
    }

    fn with(key: &str, v: Value) -> Map<String, Value> {
        let mut c = valid();
        c.insert(key.into(), v);
        c
    }

    fn without(key: &str) -> Map<String, Value> {
        let mut c = valid();
        c.remove(key);
        c
    }

    #[test]
    fn valid_token_gives_its_values() {
        assert_eq!(
            check(&valid(), &RULES, NOW),
            Some(Claimed {
                tenant_value: "acme-corp".into(),
                subject: "u-1".into(),
                act_sub: None
            })
        );
    }

    #[test]
    fn issuer_must_be_equal() {
        assert!(check(&with("iss", json!("https://p1.example/")), &RULES, NOW).is_none());
        assert!(check(&without("iss"), &RULES, NOW).is_none());
    }

    #[test]
    fn audience_string_or_list() {
        assert!(check(&with("aud", json!(["x", "strata"])), &RULES, NOW).is_some());
        assert!(check(&with("aud", json!("other")), &RULES, NOW).is_none());
        assert!(check(&with("aud", json!(["x"])), &RULES, NOW).is_none());
        assert!(check(&with("aud", json!(1)), &RULES, NOW).is_none());
        assert!(check(&without("aud"), &RULES, NOW).is_none());
    }

    #[test]
    fn exp_and_iat_are_necessary() {
        assert!(check(&without("exp"), &RULES, NOW).is_none());
        assert!(check(&without("iat"), &RULES, NOW).is_none());
        assert!(check(&with("exp", json!("soon")), &RULES, NOW).is_none());
    }

    #[test]
    fn expiry_has_60_seconds_of_tolerance() {
        let mut c = with("iat", json!(NOW - 100));
        c.insert("exp".into(), json!(NOW - 60));
        assert!(check(&c, &RULES, NOW).is_some());
        c.insert("exp".into(), json!(NOW - 61));
        assert!(check(&c, &RULES, NOW).is_none());
    }

    #[test]
    fn iat_and_nbf_in_the_future() {
        assert!(check(&with("iat", json!(NOW + 60)), &RULES, NOW).is_some());
        assert!(check(&with("iat", json!(NOW + 61)), &RULES, NOW).is_none());
        assert!(check(&with("nbf", json!(NOW + 60)), &RULES, NOW).is_some());
        assert!(check(&with("nbf", json!(NOW + 61)), &RULES, NOW).is_none());
        assert!(check(&with("nbf", json!("now")), &RULES, NOW).is_none());
    }

    #[test]
    fn lifetime_limit() {
        let mut c = with("iat", json!(NOW - 10));
        c.insert("exp".into(), json!(NOW - 10 + 3600));
        assert!(check(&c, &RULES, NOW).is_some());
        c.insert("exp".into(), json!(NOW - 10 + 3601));
        assert!(check(&c, &RULES, NOW).is_none());
    }

    #[test]
    fn tenant_and_account_claims() {
        for claim in ["org", "sub"] {
            assert!(check(&without(claim), &RULES, NOW).is_none(), "{claim}");
            for bad in [json!(""), json!(["a"]), json!(7), json!("x".repeat(257))] {
                assert!(check(&with(claim, bad), &RULES, NOW).is_none(), "{claim}");
            }
            assert!(check(&with(claim, json!("x".repeat(256))), &RULES, NOW).is_some());
        }
    }

    #[test]
    fn act_sub_is_cut_to_256_bytes() {
        let long = "é".repeat(200); // 400 bytes
        let got = check(&with("act", json!({"sub": long})), &RULES, NOW).unwrap();
        let act = got.act_sub.unwrap();
        assert!(act.len() <= 256 && long.starts_with(&act));
        assert_eq!(cut("abc", 2), "ab");
    }
}
