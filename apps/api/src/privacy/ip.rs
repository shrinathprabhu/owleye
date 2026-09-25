use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

pub fn anonymize_ip(ip: IpAddr) -> String {
    match ip {
        IpAddr::V4(value) => anonymize_ipv4(value).to_string(),
        IpAddr::V6(value) => anonymize_ipv6(value).to_string(),
    }
}

// Drop the host byte rather than retaining a rearranged/masked address such
// as 1.X.3.4. A /24 prefix is useful for coarse anonymous counting while the
// original client address cannot be reconstructed from it.
fn anonymize_ipv4(ip: Ipv4Addr) -> Ipv4Addr {
    let mut octets = ip.octets();
    octets[3] = 0;
    Ipv4Addr::from(octets)
}

// Keep only the IPv6 /64 network prefix and discard the interface identifier.
fn anonymize_ipv6(ip: Ipv6Addr) -> Ipv6Addr {
    let mut segments = ip.segments();
    segments[4] = 0;
    segments[5] = 0;
    segments[6] = 0;
    segments[7] = 0;
    Ipv6Addr::from(segments)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ipv4_discards_the_host_octet() {
        assert_eq!(anonymize_ip("203.0.113.42".parse().unwrap()), "203.0.113.0");
    }

    #[test]
    fn ipv6_discards_the_interface_identifier() {
        assert_eq!(
            anonymize_ip("2001:db8:1234:5678:90ab:cdef:1234:5678".parse().unwrap()),
            "2001:db8:1234:5678::"
        );
    }
}
