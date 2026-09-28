//! naia-lib/naia#207: one naia `Server` must be able to serve clients
//! arriving on two different transports at once, through a single `Socket`
//! handle (`MultiSocket` fan-in, `server.listen(multi)` unchanged).

#![cfg(feature = "transport_local")]

use std::net::SocketAddr;
use std::time::{Duration, Instant};

use naia_server::transport::{
    local::{LocalServerSocket, LocalTransportHub, Socket as LocalSocket},
    multi::MultiSocket,
    PacketReceiver, Socket,
};
use naia_shared::ProtocolId;

fn recv_from(receiver: &mut Box<dyn PacketReceiver>, want_addr: SocketAddr, want_payload: &[u8]) {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        match receiver.receive().expect("receiver error") {
            Some((addr, bytes)) => {
                assert_eq!(addr, want_addr, "packet arrived from the wrong client");
                assert_eq!(bytes, want_payload, "payload corrupted through fan-in");
                return;
            }
            None => {
                assert!(
                    Instant::now() < deadline,
                    "timed out waiting for packet from {want_addr}"
                );
                std::thread::sleep(Duration::from_millis(5));
            }
        }
    }
}

#[test]
fn multi_socket_serves_two_transports_through_one_handle() {
    let hub_a = LocalTransportHub::new("127.0.0.1:15001".parse().unwrap());
    let hub_b = LocalTransportHub::new("127.0.0.1:15002".parse().unwrap());

    // Client addresses are per-hub sequences (both hubs start at :12346),
    // so burn hub_b's first address to keep the two populations disjoint —
    // production transports never share a client address.
    let (addr_a, _, _, tx_a, rx_a) = hub_a.register_client();
    let _ = hub_b.register_client();
    let (addr_b, _, _, tx_b, rx_b) = hub_b.register_client();
    assert_ne!(addr_a, addr_b);

    let sock_a = LocalSocket::new(LocalServerSocket::new(hub_a), None);
    let sock_b = LocalSocket::new(LocalServerSocket::new(hub_b), None);
    let boxed: Box<dyn Socket> = MultiSocket::new(vec![Box::new(sock_a), Box::new(sock_b)]).into();
    let (_auth_sender, _auth_receiver, packet_sender, mut packet_receiver) =
        boxed.listen(ProtocolId::new(0x207));

    // A packet from client A arrives with A's address ...
    tx_a.send(vec![10, 20]).unwrap();
    recv_from(&mut packet_receiver, addr_a, &[10, 20]);
    // ... and B's packet arrives with B's address — each exactly once.
    tx_b.send(vec![30, 40]).unwrap();
    recv_from(&mut packet_receiver, addr_b, &[30, 40]);
    assert!(
        packet_receiver.receive().expect("receiver error").is_none(),
        "duplicate delivery through fan-in"
    );

    // Replies route back to the owning hub's client.
    packet_sender.send(&addr_a, &[50]).unwrap();
    assert_eq!(rx_a.recv_timeout(Duration::from_secs(5)).unwrap(), vec![50]);
    packet_sender.send(&addr_b, &[60]).unwrap();
    assert_eq!(rx_b.recv_timeout(Duration::from_secs(5)).unwrap(), vec![60]);
}
