use std::net::IpAddr;

use axum::http::HeaderMap;
use ipnet::IpNet;

/// Resolve the client address while treating forwarding headers as untrusted input
/// unless the immediate peer is explicitly configured as a trusted proxy.
pub fn client_ip_from_request(
    headers: &HeaderMap,
    peer_ip: IpAddr,
    trusted_proxies: &[IpNet],
) -> IpAddr {
    if !is_trusted_proxy(peer_ip, trusted_proxies) {
        return peer_ip;
    }

    if headers.contains_key("x-forwarded-for") {
        // Repeated field lines are one ordered chain. Looking at only the first
        // lets a client-supplied line hide the address appended by the proxy.
        let values = headers
            .get_all("x-forwarded-for")
            .iter()
            .map(|value| value.to_str())
            .collect::<Result<Vec<_>, _>>();
        return values
            .ok()
            .and_then(|values| forwarded_client_ip(&values.join(","), trusted_proxies))
            .unwrap_or(peer_ip);
    }

    header_ip(headers, "x-real-ip").unwrap_or(peer_ip)
}

fn forwarded_client_ip(value: &str, trusted_proxies: &[IpNet]) -> Option<IpAddr> {
    let addresses = value
        .split(',')
        .map(str::trim)
        .map(str::parse::<IpAddr>)
        .collect::<Result<Vec<_>, _>>()
        .ok()?;

    let mut leftmost = None;
    for address in addresses.into_iter().rev() {
        leftmost = Some(address);
        if !is_trusted_proxy(address, trusted_proxies) {
            return Some(address);
        }
    }

    leftmost
}

fn header_ip(headers: &HeaderMap, name: &str) -> Option<IpAddr> {
    let mut values = headers.get_all(name).iter();
    let value = values.next()?;
    if values.next().is_some() {
        return None;
    }
    value.to_str().ok()?.parse::<IpAddr>().ok()
}

fn is_trusted_proxy(address: IpAddr, trusted_proxies: &[IpNet]) -> bool {
    trusted_proxies
        .iter()
        .any(|network| network.contains(&address))
}

#[cfg(test)]
mod tests {
    use axum::http::HeaderValue;

    use super::*;

    fn loopback_proxies() -> Vec<IpNet> {
        vec!["127.0.0.0/8".parse().unwrap(), "::1/128".parse().unwrap()]
    }

    #[test]
    fn ignores_forwarding_headers_from_untrusted_peers() {
        let mut headers = HeaderMap::new();
        headers.insert("x-forwarded-for", HeaderValue::from_static("203.0.113.8"));

        let peer = "198.51.100.4".parse().unwrap();
        assert_eq!(
            client_ip_from_request(&headers, peer, &loopback_proxies()),
            peer
        );
    }

    #[test]
    fn returns_first_untrusted_address_from_right_of_forwarded_chain() {
        let mut headers = HeaderMap::new();
        headers.insert(
            "x-forwarded-for",
            HeaderValue::from_static("203.0.113.8, 198.51.100.9, 127.0.0.2"),
        );

        assert_eq!(
            client_ip_from_request(&headers, "127.0.0.1".parse().unwrap(), &loopback_proxies()),
            "198.51.100.9".parse::<IpAddr>().unwrap()
        );
    }

    #[test]
    fn repeated_forwarded_lines_use_the_complete_chain() {
        let mut headers = HeaderMap::new();
        headers.append("x-forwarded-for", HeaderValue::from_static("203.0.113.8"));
        headers.append(
            "x-forwarded-for",
            HeaderValue::from_static("198.51.100.9, 127.0.0.2"),
        );
        assert_eq!(
            client_ip_from_request(&headers, "127.0.0.1".parse().unwrap(), &loopback_proxies()),
            "198.51.100.9".parse::<IpAddr>().unwrap()
        );
    }

    #[test]
    fn malformed_or_ambiguous_headers_never_fall_through_to_another_claim() {
        let peer = "127.0.0.1".parse().unwrap();
        for invalid in [
            HeaderValue::from_static("unknown"),
            HeaderValue::from_bytes(&[0xff]).unwrap(),
        ] {
            let mut headers = HeaderMap::new();
            headers.append("x-forwarded-for", HeaderValue::from_static("203.0.113.8"));
            headers.append("x-forwarded-for", invalid);
            headers.insert("x-real-ip", HeaderValue::from_static("203.0.113.9"));
            assert_eq!(
                client_ip_from_request(&headers, peer, &loopback_proxies()),
                peer
            );
        }
        let mut headers = HeaderMap::new();
        headers.append("x-real-ip", HeaderValue::from_static("203.0.113.8"));
        headers.append("x-real-ip", HeaderValue::from_static("203.0.113.9"));
        assert_eq!(
            client_ip_from_request(&headers, peer, &loopback_proxies()),
            peer
        );
    }

    #[test]
    fn malformed_forwarded_chain_falls_back_to_peer() {
        let mut headers = HeaderMap::new();
        headers.insert(
            "x-forwarded-for",
            HeaderValue::from_static("203.0.113.8, unknown"),
        );

        let peer = "127.0.0.1".parse().unwrap();
        assert_eq!(
            client_ip_from_request(&headers, peer, &loopback_proxies()),
            peer
        );
    }
}
