//! Public-internet HEAD probes. DNS is resolved once, validated, then pinned
//! into a fresh client; redirects and environment proxies cannot bypass it.
use crate::ApiError;
use reqwest::{Client, Url};
use serde::{Deserialize, Serialize};
use std::{
    net::{IpAddr, SocketAddr},
    time::{Duration, Instant},
};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(super) struct Observation {
    pub status_code: Option<u16>,
    pub duration_ms: u64,
    pub failure: Option<String>,
}
impl Observation {
    pub fn healthy(&self) -> bool {
        self.status_code == Some(200) && self.failure.is_none()
    }
    fn failed(reason: &str, start: Instant) -> Self {
        Self {
            status_code: None,
            duration_ms: start.elapsed().as_millis() as u64,
            failure: Some(reason.into()),
        }
    }
}

pub(super) fn parse_url(input: &str) -> Result<Url, ApiError> {
    let invalid = || {
        ApiError::BadRequest("Use a public HTTP(S) URL on port 80 or 443, without credentials, query parameters, or fragments".into())
    };
    if input.len() > 2048 || input.chars().any(char::is_control) {
        return Err(invalid());
    }
    let url = Url::parse(input.trim()).map_err(|_| invalid())?;
    if !matches!(url.scheme(), "http" | "https")
        || !matches!(url.port_or_known_default(), Some(80 | 443))
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
        || url.host_str().is_none()
    {
        return Err(invalid());
    }
    let host = url.host_str().unwrap().trim_matches(['[', ']']);
    if host.eq_ignore_ascii_case("localhost")
        || host.to_ascii_lowercase().ends_with(".localhost")
        || host.parse::<IpAddr>().is_ok_and(|ip| !public_ip(ip))
    {
        return Err(invalid());
    }
    Ok(url)
}

pub(super) fn public_ip(ip: IpAddr) -> bool {
    let denied: &[&str] = match ip {
        IpAddr::V4(_) => &[
            "0.0.0.0/8",
            "10.0.0.0/8",
            "100.64.0.0/10",
            "127.0.0.0/8",
            "169.254.0.0/16",
            "172.16.0.0/12",
            "192.0.0.0/24",
            "192.0.2.0/24",
            "192.88.99.0/24",
            "192.168.0.0/16",
            "198.18.0.0/15",
            "198.51.100.0/24",
            "203.0.113.0/24",
            "224.0.0.0/3",
        ],
        IpAddr::V6(v6) => {
            // Only ordinary global unicast. Excludes mapped IPv4, NAT64,
            // link-local, unique-local, multicast, Teredo and 6to4 tunnels.
            if !"2000::/3"
                .parse::<ipnet::IpNet>()
                .unwrap()
                .contains(&IpAddr::V6(v6))
            {
                return false;
            }
            &["2001::/23", "2001:db8::/32", "2002::/16", "3fff::/20"]
        }
    };
    !denied
        .iter()
        .any(|range| range.parse::<ipnet::IpNet>().unwrap().contains(&ip))
}

pub(super) async fn check(input: &str) -> Observation {
    let start = Instant::now();
    match tokio::time::timeout(Duration::from_secs(15), check_inner(input, start)).await {
        Ok(result) => result,
        Err(_) => Observation::failed("timeout", start),
    }
}
async fn check_inner(input: &str, start: Instant) -> Observation {
    let Ok(url) = parse_url(input) else {
        return Observation::failed("blocked_destination", start);
    };
    let host = url.host_str().unwrap().trim_matches(['[', ']']);
    let port = url.port_or_known_default().unwrap();
    let addresses: Vec<SocketAddr> = match tokio::net::lookup_host((host, port)).await {
        Ok(addresses) => addresses.collect(),
        Err(_) => return Observation::failed("dns", start),
    };
    if addresses.is_empty() {
        return Observation::failed("dns", start);
    }
    // Reject mixed public/private answers, not just the first result.
    if addresses.iter().any(|address| !public_ip(address.ip())) {
        return Observation::failed("blocked_destination", start);
    }
    let client = match Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .connect_timeout(Duration::from_secs(5))
        .timeout(Duration::from_secs(10))
        .user_agent("OwlEye-Uptime/1.0")
        .resolve_to_addrs(host, &addresses)
        .build()
    {
        Ok(client) => client,
        Err(_) => return Observation::failed("checker_unavailable", start),
    };
    send_head(&client, url, start).await
}
async fn send_head(client: &Client, url: Url, start: Instant) -> Observation {
    match client.head(url).send().await {
        Ok(response) => {
            let status = response.status().as_u16();
            Observation {
                status_code: Some(status),
                duration_ms: start.elapsed().as_millis() as u64,
                failure: (status != 200).then(|| "http_status".into()),
            }
        }
        Err(error) => Observation::failed(
            if error.is_timeout() {
                "timeout"
            } else {
                "connection_or_tls"
            },
            start,
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_unsafe_urls_and_special_addresses() {
        for url in [
            "file:///etc/passwd",
            "http://127.1/",
            "http://2130706433/",
            "http://[::1]/",
            "https://a:b@example.com/",
            "https://example.com/?token=x",
            "https://example.com/#x",
            "http://example.com:22/",
            "http://localhost/",
        ] {
            assert!(parse_url(url).is_err(), "{url}");
        }
        for ip in [
            "10.1.1.1",
            "169.254.169.254",
            "100.100.100.200",
            "192.168.1.1",
            "198.18.0.1",
            "240.0.0.1",
            "::ffff:8.8.8.8",
            "64:ff9b::808:808",
            "2002:0808:0808::1",
            "2001:db8::1",
            "fe80::1",
            "fc00::1",
        ] {
            assert!(!public_ip(ip.parse().unwrap()), "{ip}");
        }
        for ip in ["1.1.1.1", "8.8.8.8", "2606:4700:4700::1111"] {
            assert!(public_ip(ip.parse().unwrap()));
        }
        assert!(parse_url("https://example.com/health").is_ok());
    }
    #[tokio::test]
    async fn head_only_exact_200_and_no_redirects() {
        use axum::{http::StatusCode, routing::head, Router};
        let app = Router::new()
            .route("/ok", head(|| async { StatusCode::OK }))
            .route("/empty", head(|| async { StatusCode::NO_CONTENT }))
            .route(
                "/redirect",
                head(|| async { axum::response::Redirect::temporary("/ok") }),
            )
            .route(
                "/unsupported",
                head(|| async { StatusCode::METHOD_NOT_ALLOWED }),
            );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let task = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        // Only the transport helper can use loopback in a test; public check()
        // retains exactly the same destination restrictions in every build.
        let client = Client::builder()
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .unwrap();
        for (path, code) in [
            ("ok", 200),
            ("empty", 204),
            ("redirect", 307),
            ("unsupported", 405),
        ] {
            let result = send_head(
                &client,
                Url::parse(&format!("http://{addr}/{path}")).unwrap(),
                Instant::now(),
            )
            .await;
            assert_eq!(result.status_code, Some(code));
            assert_eq!(result.healthy(), code == 200);
        }
        assert_eq!(
            check(&format!("http://{addr}/ok")).await.failure.as_deref(),
            Some("blocked_destination")
        );
        task.abort();
    }
}
