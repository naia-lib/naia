use std::net::SocketAddr;
use std::sync::{mpsc, Arc, Mutex};
use std::time::Duration;

use naia_shared::{IdentityToken, Protocol};

use crate::transport::{
    IdentityReceiver, IdentityReceiverResult, PacketReceiver, PacketSender, RecvError, SendError,
    ServerAddr,
};
use crate::world_events::ErrorEvent;

use super::*;

fn dummy_server() -> ServerAddr {
    ServerAddr::Found("127.0.0.1:9999".parse::<SocketAddr>().unwrap())
}

#[derive(Clone)]
struct OkIdReceiver;

impl IdentityReceiver for OkIdReceiver {
    fn receive(&mut self) -> IdentityReceiverResult {
        IdentityReceiverResult::Success(IdentityToken::generate())
    }
}

#[derive(Clone)]
struct OkPacketSender;

impl PacketSender for OkPacketSender {
    fn send(&self, _payload: &[u8]) -> Result<(), SendError> {
        Ok(())
    }

    fn server_addr(&self) -> ServerAddr {
        dummy_server()
    }

    fn shutdown(&mut self) {}
}

#[derive(Clone)]
struct FailPacketReceiver;

impl PacketReceiver for FailPacketReceiver {
    fn receive(&mut self) -> Result<Option<&[u8]>, RecvError> {
        Err(RecvError)
    }

    fn server_addr(&self) -> ServerAddr {
        dummy_server()
    }
}

#[test]
fn maintain_handshake_terminates_on_persistent_transport_error() {
    let mut client = Client::<u64>::new(ClientConfig::default(), Protocol::builder().build());
    client.io.load(
        Box::new(OkIdReceiver),
        Box::new(OkPacketSender),
        Box::new(FailPacketReceiver),
    );

    let client = Arc::new(Mutex::new(client));
    let worker = client.clone();
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        worker.lock().unwrap().maintain_handshake();
        let _ = tx.send(());
    });
    rx.recv_timeout(Duration::from_secs(10))
        .expect("maintain_handshake must return on a persistently-failing transport");
    // The loop hit its Err arm (not skipped): exactly one error.
    let errors = client
        .lock()
        .unwrap()
        .incoming_world_events
        .read::<ErrorEvent>()
        .count();
    assert_eq!(
        errors, 1,
        "one error per maintain_handshake call, not one per spin"
    );
}
