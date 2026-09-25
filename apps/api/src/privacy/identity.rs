use std::net::IpAddr;

use chrono::{DateTime, Utc};

use super::ip::anonymize_ip;

pub struct AnonymousIds {
    pub session_id: String,
    pub user_id: String,
    pub visitor_id: String,
}

pub fn anonymous_ids(
    site_id: &str,
    ip: IpAddr,
    user_agent: Option<&str>,
    timezone: Option<&str>,
    salt: &str,
    occurred_at: DateTime<Utc>,
) -> AnonymousIds {
    let day = occurred_at.format("%Y-%m-%d").to_string();
    let anonymized_ip = anonymize_ip(ip);
    let visitor_id = hash_parts(
        salt,
        "visitor-v1",
        &[
            site_id,
            &anonymized_ip,
            user_agent.unwrap_or_default(),
            timezone.unwrap_or_default(),
        ],
    );
    let user_id = hash_parts(salt, "daily-user-v1", &[site_id, &visitor_id, &day]);
    let session_bucket = (occurred_at.timestamp() / 1_800).to_string();
    let session_id = hash_parts(salt, "session-v1", &[site_id, &visitor_id, &session_bucket]);

    AnonymousIds {
        session_id,
        user_id,
        visitor_id,
    }
}

/// Produces a short-lived in-memory rate-limit key without retaining the raw
/// address in the limiter map. Unlike visitor IDs this intentionally uses the
/// complete address so unrelated clients on the same subnet do not share a
/// rate-limit bucket.
pub fn anonymous_rate_limit_key(ip: IpAddr, salt: &str) -> String {
    let ip = ip.to_string();
    hash_parts(salt, "rate-limit-v1", &[&ip])
}

fn hash_parts(salt: &str, domain: &str, parts: &[&str]) -> String {
    let key = blake3::derive_key("owleye privacy identifiers v1", salt.as_bytes());
    let mut hasher = blake3::Hasher::new_keyed(&key);

    hasher.update(domain.as_bytes());
    hasher.update(&[0]);
    for part in parts {
        hasher.update(part.as_bytes());
        hasher.update(&[0]);
    }

    hasher.finalize().to_hex().to_string()
}

#[cfg(test)]
mod tests {
    use chrono::{TimeZone, Utc};

    use super::*;

    const SALT: &str = "a-secret-long-enough-for-identity-tests";

    fn at(day: u32) -> chrono::DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 8, day, 12, 15, 0).unwrap()
    }

    #[test]
    fn visitors_are_site_scoped_and_stable_across_days() {
        let first = anonymous_ids(
            "site_one",
            "203.0.113.42".parse().unwrap(),
            Some("Browser 1"),
            Some("Asia/Kolkata"),
            SALT,
            at(1),
        );
        let next_day = anonymous_ids(
            "site_one",
            "203.0.113.42".parse().unwrap(),
            Some("Browser 1"),
            Some("Asia/Kolkata"),
            SALT,
            at(2),
        );
        let another_site = anonymous_ids(
            "site_two",
            "203.0.113.42".parse().unwrap(),
            Some("Browser 1"),
            Some("Asia/Kolkata"),
            SALT,
            at(1),
        );

        assert_eq!(first.visitor_id, next_day.visitor_id);
        assert_ne!(first.user_id, next_day.user_id);
        assert_ne!(first.visitor_id, another_site.visitor_id);
    }

    #[test]
    fn ipv4_host_byte_is_discarded_before_visitor_hashing() {
        let first = anonymous_ids(
            "site_one",
            "203.0.113.42".parse().unwrap(),
            None,
            None,
            SALT,
            at(1),
        );
        let same_subnet = anonymous_ids(
            "site_one",
            "203.0.113.199".parse().unwrap(),
            None,
            None,
            SALT,
            at(1),
        );

        assert_eq!(first.visitor_id, same_subnet.visitor_id);
    }

    #[test]
    fn rate_limit_keys_do_not_expose_addresses() {
        let address: IpAddr = "203.0.113.42".parse().unwrap();
        let key = anonymous_rate_limit_key(address, SALT);

        assert!(!key.contains("203.0.113.42"));
        assert_eq!(key.len(), 64);
        assert_eq!(key, anonymous_rate_limit_key(address, SALT));
    }
}
