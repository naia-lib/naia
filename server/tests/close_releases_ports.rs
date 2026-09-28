//! naia#92: closing a server releases its ports — dropping the listen
//! handles frees the UDP socket's ports and the WebRTC socket's ports
//! alike, and both rebind deterministically.

#![cfg(all(feature = "transport_udp", feature = "transport_webrtc"))]

use std::{
    net::SocketAddr,
    time::{Duration, Instant},
};

use naia_server::transport::{
    udp::{ServerAddrs as UdpAddrs, Socket as UdpSocket},
    webrtc::{ServerAddrs as RtcAddrs, Socket as RtcSocket},
    Socket as TransportSocket,
};
use naia_shared::{ProtocolId, SocketConfig};

const UDP_AUTH_PORT: u16 = 15581;
const UDP_DATA_PORT: u16 = 15582;
const RTC_SESSION_PORT: u16 = 15583;
const RTC_DATA_PORT: u16 = 15584;

fn udp_addrs() -> UdpAddrs {
    UdpAddrs::new(
        SocketAddr::from(([127, 0, 0, 1], UDP_AUTH_PORT)),
        SocketAddr::from(([127, 0, 0, 1], UDP_DATA_PORT)),
        &format!("http://127.0.0.1:{UDP_DATA_PORT}"),
    )
}

fn rtc_addrs() -> RtcAddrs {
    RtcAddrs::new(
        SocketAddr::from(([127, 0, 0, 1], RTC_SESSION_PORT)),
        SocketAddr::from(([127, 0, 0, 1], RTC_DATA_PORT)),
        &format!("http://127.0.0.1:{RTC_DATA_PORT}"),
    )
}

fn udp_taken(addr: SocketAddr) -> bool {
    match std::net::UdpSocket::bind(addr) {
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

/// The WebRTC socket binds on detached tasks: wait until its ports are
/// taken (proves the release below is real, not vacuous). The UDP socket
/// binds synchronously at construction, so no wait is needed there.
fn wait_until_rtc_bound(rtc: &RtcAddrs) {
    let start = Instant::now();
    while start.elapsed() < Duration::from_secs(30) {
        if tcp_taken(rtc.session_listen_addr) && udp_taken(rtc.webrtc_listen_addr) {
            return;
        }
        std::thread::sleep(Duration::from_millis(25));
    }
    panic!("webrtc socket never bound its ports");
}

type ListenHandles = (
    Box<dyn naia_server::transport::AuthSender>,
    Box<dyn naia_server::transport::AuthReceiver>,
    Box<dyn naia_server::transport::PacketSender>,
    Box<dyn naia_server::transport::PacketReceiver>,
);

fn listen_each(udp: &UdpAddrs, rtc: &RtcAddrs) -> (ListenHandles, ListenHandles) {
    let udp_socket = UdpSocket::new(udp, None);
    let boxed_udp: Box<dyn TransportSocket> = Box::new(udp_socket);
    let rtc_socket = RtcSocket::new(rtc, &SocketConfig::default());
    let boxed_rtc: Box<dyn TransportSocket> = Box::new(rtc_socket);
    (
        boxed_udp.listen(ProtocolId::new(0x92)),
        boxed_rtc.listen(ProtocolId::new(0x92)),
    )
}

#[test]
fn closing_each_server_releases_its_ports() {
    let udp = udp_addrs();
    let rtc = rtc_addrs();

    let (udp_handles, rtc_handles) = listen_each(&udp, &rtc);
    wait_until_rtc_bound(&rtc);

    // Dropping each socket's handles frees that socket's ports.
    drop(udp_handles);
    drop(rtc_handles);

    // WebRTC: explicit observable wait; UDP rebinds at once (the probe
    // socket proves freeness, then drops out of scope so the re-listen
    // below starts from free ports).
    assert!(
        rtc.wait_until_free(Duration::from_secs(10)),
        "webrtc ports not free 10s after dropping handles"
    );
    {
        let _udp_probe = UdpSocket::new(&udp, None);
    }

    // And each listen rebinds deterministically on the same addresses —
    // full close/reopen cycle with no sleeps past the waits.
    let (udp_handles, rtc_handles) = listen_each(&udp, &rtc);
    wait_until_rtc_bound(&rtc);
    drop(udp_handles);
    drop(rtc_handles);
    assert!(
        rtc.wait_until_free(Duration::from_secs(10)),
        "webrtc ports not free 10s after second close"
    );
    {
        let _udp_probe = UdpSocket::new(&udp, None);
    }
}
