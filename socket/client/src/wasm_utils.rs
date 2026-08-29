use std::net::{IpAddr, SocketAddr};

use log::warn;

use super::server_addr::ServerAddr;

// ICE candidate format (RFC 8839 §5.1):
//   "candidate:<foundation> <component> <transport> <priority> <addr> <port> typ <type> ..."
// Fields are space-separated. We scan for the first adjacent (address, port)
// pair rather than indexing fixed offsets, which keeps this working on lines
// whose leading fields differ, and matches how webrtc-unreliable-client parses
// the same string on the native side.
//
// Using a regex for this pulled in the full regex + aho-corasick crates
// (~200 KB wasm code).
pub fn candidate_to_addr(candidate_str: &str) -> ServerAddr {
    let tokens: Vec<&str> = candidate_str.split_whitespace().collect();
    for w in tokens.windows(2) {
        if let Ok(ip_addr) = w[0].parse::<IpAddr>() {
            if let Ok(port) = w[1].parse::<u16>() {
                return ServerAddr::Found(SocketAddr::new(ip_addr, port));
            }
        }
    }

    // This used to panic. A malformed candidate is the server's doing, not the
    // application's, and taking down the browser tab over it helps nobody --
    // the connection simply never finds an address.
    warn!("no SocketAddr found in ICE candidate: {candidate_str}");
    ServerAddr::Finding
}

#[cfg(test)]
mod tests {
    use super::*;

    fn found(addr: &str) -> ServerAddr {
        ServerAddr::Found(addr.parse().unwrap())
    }

    #[test]
    fn reads_the_address_out_of_a_host_candidate() {
        assert_eq!(
            candidate_to_addr("candidate:1 1 UDP 1755993416 127.0.0.1 14192 typ host"),
            found("127.0.0.1:14192")
        );
    }

    #[test]
    fn reads_an_ipv6_address() {
        assert_eq!(
            candidate_to_addr("candidate:1 1 UDP 1755993416 ::1 14192 typ host"),
            found("[::1]:14192")
        );
    }

    #[test]
    fn parses_an_srflx_candidate_with_a_raddr_tail() {
        // Server-reflexive candidates carry a longer tail than host ones. The
        // raddr address in it is followed by the literal "rport", so it never
        // forms a second (ip, port) pair for the scan to trip over.
        assert_eq!(
            candidate_to_addr(
                "candidate:842163049 1 udp 1686052607 10.0.0.5 51234 typ srflx raddr 0.0.0.0 rport 0"
            ),
            found("10.0.0.5:51234")
        );
    }

    #[test]
    fn reports_finding_when_there_is_no_address() {
        // These used to panic. Returning Finding leaves the caller looking,
        // which is what it would be doing anyway.
        assert_eq!(
            candidate_to_addr("candidate:1 1 UDP 1755993416 typ host"),
            ServerAddr::Finding
        );
        assert_eq!(candidate_to_addr(""), ServerAddr::Finding);
        assert_eq!(
            candidate_to_addr("not a candidate line at all"),
            ServerAddr::Finding
        );
    }

    #[test]
    fn reports_finding_when_the_port_is_not_a_port() {
        assert_eq!(
            candidate_to_addr("candidate:1 1 UDP 1755993416 127.0.0.1 99999 typ host"),
            ServerAddr::Finding
        );
    }
}
