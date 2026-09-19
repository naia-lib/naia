//! naia-lib/naia#92: dropping the server socket handles must release the
//! bound ports so a new `Socket::listen` on the same addresses succeeds.

use std::{
    net::{SocketAddr, UdpSocket},
    time::{Duration, Instant},
};

use naia_server_socket::{ServerAddrs, Socket};
use naia_socket_shared::SocketConfig;

const PROTOCOL_ID: &str = "0000000000000000000000000000d000";
const SESSION_PORT: u16 = 15491;
const WEBRTC_PORT: u16 = 15492;

fn addrs() -> ServerAddrs {
    ServerAddrs::new(
        SocketAddr::from(([127, 0, 0, 1], SESSION_PORT)),
        SocketAddr::from(([127, 0, 0, 1], WEBRTC_PORT)),
        &format!("http://127.0.0.1:{WEBRTC_PORT}"),
    )
}

fn udp_taken(addr: SocketAddr) -> bool {
    match UdpSocket::bind(addr) {
        Ok(_) => false,
        Err(e) if e.kind() == std::io::ErrorKind::AddrInUse => true,
        Err(_) => false,
    }
}

fn tcp_taken(addr: SocketAddr) -> bool {
    match std::net::TcpListener::bind(addr) {
        Ok(_) => false,
        Err(e) if e.kind() == std::io::ErrorKind::AddrInUse => true,
        Err(_) => false,
    }
}

fn wait_for(
    check: impl Fn(SocketAddr) -> bool,
    addr: SocketAddr,
    taken: bool,
    timeout: Duration,
) -> bool {
    let start = Instant::now();
    while start.elapsed() < timeout {
        if check(addr) == taken {
            return true;
        }
        std::thread::sleep(Duration::from_millis(25));
    }
    check(addr) == taken
}

#[test]
fn dropping_socket_handles_releases_listen_ports() {
    let server_addrs = addrs();
    let session_addr = server_addrs.session_listen_addr;
    let webrtc_addr = server_addrs.webrtc_listen_addr;

    // Sanity: ports start free (session listener is TCP, webrtc is UDP).
    assert!(!tcp_taken(session_addr), "session port busy before listen");
    assert!(!udp_taken(webrtc_addr), "webrtc port busy before listen");

    let (packet_sender, packet_receiver) =
        Socket::listen(&server_addrs, &SocketConfig::default(), PROTOCOL_ID);

    // The binds happen on detached tasks: wait until both ports are taken.
    assert!(
        wait_for(udp_taken, webrtc_addr, true, Duration::from_secs(30)),
        "webrtc UDP port was never bound"
    );
    assert!(
        wait_for(tcp_taken, session_addr, true, Duration::from_secs(30)),
        "session TCP port was never bound"
    );

    drop(packet_sender);
    drop(packet_receiver);

    // Both ports must come back so a fresh listen can bind them.
    assert!(
        wait_for(udp_taken, webrtc_addr, false, Duration::from_secs(10)),
        "webrtc UDP port still bound 10s after dropping socket handles (#92)"
    );
    assert!(
        wait_for(tcp_taken, session_addr, false, Duration::from_secs(10)),
        "session TCP port still bound 10s after dropping socket handles (#92)"
    );
}
