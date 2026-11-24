//! The status routes of tech design 0047: the public status of a revision, one revision or a
//! batch, with `Retry-After` for a status that is not final.

/// The setting of the `Retry-After` of a status that is not final, in seconds.
pub const RETRY_AFTER_VAR: &str = "STRATA_STATUS_RETRY_AFTER";
/// The default of 0047.
pub const DEFAULT_RETRY_AFTER: u64 = 2;

/// The value of `STRATA_STATUS_RETRY_AFTER`: whole seconds from 1 to 3,600, and the default when
/// the variable is absent. Each other value is an error that names the variable.
pub fn retry_after(value: Option<&str>) -> Result<u64, String> {
    let Some(v) = value else {
        return Ok(DEFAULT_RETRY_AFTER);
    };
    match v.parse::<u64>() {
        Ok(n @ 1..=3600) => Ok(n),
        _ => Err(format!(
            "{RETRY_AFTER_VAR}: {v:?} is not whole seconds from 1 to 3600"
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_setting_and_its_limits() {
        assert_eq!(retry_after(None), Ok(2));
        assert_eq!(retry_after(Some("1")), Ok(1));
        assert_eq!(retry_after(Some("3600")), Ok(3600));
        for bad in ["0", "3601", "x", "-1", ""] {
            assert!(
                retry_after(Some(bad))
                    .unwrap_err()
                    .contains(RETRY_AFTER_VAR)
            );
        }
    }
}
