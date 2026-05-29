use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};

use super::server_addr::ServerAddr;

// ICE candidate format (RFC 8839 §5.1):
//   "candidate:<foundation> <component> <transport> <priority> <addr> <port> typ <type> ..."
// Fields are space-separated; address is at offset 4, port at offset 5.
// Using a regex for this pulled in the full regex + aho-corasick crates (~200 KB wasm code).
pub fn candidate_to_addr(candidate_str: &str) -> ServerAddr {
    let mut parts = candidate_str.split_whitespace();
    let _ = parts.next(); // candidate:<foundation>
    let _ = parts.next(); // component
    let _ = parts.next(); // transport
    let _ = parts.next(); // priority
    let addr_str = parts.next().expect("missing address in ICE candidate");
    let port_str = parts.next().expect("missing port in ICE candidate");

    let port: u16 = port_str.parse().expect("not a valid port");
    if let Ok(ip_addr) = addr_str.parse::<Ipv6Addr>() {
        ServerAddr::Found(SocketAddr::new(IpAddr::V6(ip_addr), port))
    } else {
        let ip_addr = addr_str
            .parse::<Ipv4Addr>()
            .expect("not a valid ip address");
        ServerAddr::Found(SocketAddr::new(IpAddr::V4(ip_addr), port))
    }
}

#[cfg(test)]
mod tests {

    use crate::{server_addr::ServerAddr, wasm_utils::candidate_to_addr};
    use std::net::{IpAddr, Ipv4Addr, SocketAddr};

    #[test]
    fn candidate_to_addr_works() {
        assert_eq!(
            candidate_to_addr("candidate:1 1 UDP 1755993416 127.0.0.1 14192 typ host"),
            ServerAddr::Found(SocketAddr::new(
                IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)),
                14192
            ))
        );
    }
}
