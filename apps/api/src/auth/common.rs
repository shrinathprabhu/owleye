use std::time::{SystemTime, UNIX_EPOCH};

use chrono::{DateTime, Utc};
use rand::{distributions::Alphanumeric, Rng};

use crate::ApiError;

pub(super) const LOGIN_CHALLENGE_TTL_MINUTES: i64 = 5;

pub(super) fn encode(value: &str) -> String {
    urlencoding::encode(value).into_owned()
}

pub(crate) fn hash_secret(salt: &str, parts: &[&str]) -> String {
    let mut hasher = blake3::Hasher::new();
    hasher.update(salt.as_bytes());

    for part in parts {
        hasher.update(&[0]);
        hasher.update(part.as_bytes());
    }

    hasher.finalize().to_hex().to_string()
}

pub(crate) fn constant_time_eq(left: &[u8], right: &[u8]) -> bool {
    if left.len() != right.len() {
        return false;
    }
    left.iter()
        .zip(right)
        .fold(0_u8, |difference, (left, right)| {
            difference | (left ^ right)
        })
        == 0
}

pub(crate) fn normalize_email(email: &str) -> Result<String, ApiError> {
    let invalid = || ApiError::BadRequest("A valid email is required".to_owned());
    let email = email.trim();
    if email.len() > 1024 {
        return Err(invalid());
    }
    let (local, domain) = email.rsplit_once('@').ok_or_else(invalid)?;
    // Domain normalization is IDNA/UTS #46, never Punycode the mailbox name.
    // Preserve Unicode mailbox spelling; retain existing ASCII case behavior.
    let local = local.to_ascii_lowercase();
    let domain = idna::domain_to_ascii_strict(domain).map_err(|_| invalid())?;
    static LOCAL_PART: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| {
        regex::Regex::new(r"^(?:[a-z0-9.!#$%&'*+/=?^_`{|}~-]|[^\x00-\x7F\p{C}\p{Z}])+$")
            .expect("valid email pattern")
    });
    let valid = local.len() <= 64
        && !local.starts_with('.')
        && !local.ends_with('.')
        && !local.contains("..")
        && LOCAL_PART.is_match(&local)
        && domain.contains('.')
        && domain.split('.').all(|label| {
            !label.is_empty()
                && label.len() <= 63
                && !label.starts_with('-')
                && !label.ends_with('-')
                && label
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
        })
        && domain
            .rsplit('.')
            .next()
            .is_some_and(|label| label.bytes().any(|byte| byte.is_ascii_alphabetic()));
    let email = format!("{local}@{domain}");
    if email.len() > 254 || !valid {
        return Err(ApiError::BadRequest("A valid email is required".to_owned()));
    }

    Ok(email)
}

pub(super) fn parse_time(value: &str) -> Result<DateTime<Utc>, ApiError> {
    Ok(DateTime::parse_from_rfc3339(value)?.with_timezone(&Utc))
}

pub(super) fn random_token(len: usize) -> String {
    rand::thread_rng()
        .sample_iter(&Alphanumeric)
        .take(len)
        .map(char::from)
        .collect()
}

pub(super) fn unix_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or_default()
}
