//! naia#92: dropping a UDP transport's handles must release its ports so a
//! new listen on the same addresses succeeds.

#![cfg(feature = "transport_udp")]

use std::net::SocketAddr;

use naia_server::transport::{
    udp::{ServerAddrs, Socket},
    Socket as TransportSocket,
};
use naia_shared::ProtocolId;

const AUTH_PORT: u16 = 15591;
const UDP_PORT: u16 = 15592;

fn addrs() -> ServerAddrs {
    ServerAddrs::new(
        SocketAddr::from(([127, 0, 0, 1], AUTH_PORT)),
        SocketAddr::from(([127, 0, 0, 1], UDP_PORT)),
        &format!("http://127.0.0.1:{UDP_PORT}"),
    )
}

#[test]
fn dropping_udp_transport_releases_ports() {
    let addrs = addrs();

    // First bind takes both ports (auth TCP + data UDP).
    let sock = Socket::new(&addrs, None);
    let handles = Box::new(sock).listen(ProtocolId::new(0x92));

    // Dropping every handle must free them: a second bind on the same
    // addresses succeeds (`Socket::new` unwraps the binds, so a leak
    // fails here with AddrInUse).
    drop(handles);
    let _again = Socket::new(&addrs, None);
}
