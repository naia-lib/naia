//! naia-lib/naia#92: dropping the server socket handles must release the
//! bound ports so a new `Socket::listen` on the same addresses succeeds,
//! and the explicit `Socket::close` must make that deterministic — it
//! returns only once the ports are free, so the rebind needs no sleeps.

use std::{
    net::{SocketAddr, UdpSocket},
    time::{Duration, Instant},
};

use naia_server_socket::{ServerAddrs, Socket};
use naia_socket_shared::SocketConfig;

const PROTOCOL_ID: &str = "0000000000000000000000000000d000";
// Each test gets its own ports: cargo runs them in parallel.
const DROP_SESSION_PORT: u16 = 15491;
const DROP_WEBRTC_PORT: u16 = 15492;
const CLOSE_SESSION_PORT: u16 = 15493;
const CLOSE_WEBRTC_PORT: u16 = 15494;
const AUTH_SESSION_PORT: u16 = 15495;
const AUTH_WEBRTC_PORT: u16 = 15496;

fn addrs(session_port: u16, webrtc_port: u16) -> ServerAddrs {
    ServerAddrs::new(
        SocketAddr::from(([127, 0, 0, 1], session_port)),
        SocketAddr::from(([127, 0, 0, 1], webrtc_port)),
        &format!("http://127.0.0.1:{webrtc_port}"),
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
    let server_addrs = addrs(DROP_SESSION_PORT, DROP_WEBRTC_PORT);
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

/// Explicit close ends the detached tasks and reports only once both
/// ports are free — the rebind below takes no sleep and no poll loop.
#[test]
fn closing_socket_handles_releases_listen_ports() {
    let server_addrs = addrs(CLOSE_SESSION_PORT, CLOSE_WEBRTC_PORT);

    let handles = Socket::listen(&server_addrs, &SocketConfig::default(), PROTOCOL_ID);

    assert!(
        Socket::close(handles, &server_addrs, Duration::from_secs(10)),
        "ports not free 10s after Socket::close (#92)"
    );

    // Immediate rebind proves the release is deterministic, not eventual.
    let handles = Socket::listen(&server_addrs, &SocketConfig::default(), PROTOCOL_ID);
    assert!(
        Socket::close(handles, &server_addrs, Duration::from_secs(10)),
        "ports not free 10s after second Socket::close (#92)"
    );
}

/// Same contract for the auth listen shape (four handles).
#[test]
fn closing_auth_socket_handles_releases_listen_ports() {
    let server_addrs = addrs(AUTH_SESSION_PORT, AUTH_WEBRTC_PORT);

    let handles = Socket::listen_with_auth(&server_addrs, &SocketConfig::default(), PROTOCOL_ID);

    assert!(
        Socket::close_with_auth(handles, &server_addrs, Duration::from_secs(10)),
        "ports not free 10s after Socket::close_with_auth (#92)"
    );

    let handles = Socket::listen_with_auth(&server_addrs, &SocketConfig::default(), PROTOCOL_ID);
    assert!(
        Socket::close_with_auth(handles, &server_addrs, Duration::from_secs(10)),
        "ports not free 10s after second Socket::close_with_auth (#92)"
    );
}
