use std::net::SocketAddr;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use naia_shared::{
    ComponentKind, IdentityToken, Protocol, ReplicaDynRefWrapper, ReplicaRefWrapper,
    ReplicatedComponent, WorldRefType,
};

use crate::transport::{
    IdentityReceiver, IdentityReceiverResult, PacketReceiver, PacketSender, RecvError, SendError,
    ServerAddr,
};

use super::*;

/// Empty world: the handshake send branch never touches it, but the
/// `send_all_packets` bound still needs a value.
struct StubWorld;

impl WorldRefType<u64> for StubWorld {
    fn has_entity(&self, _world_entity: &u64) -> bool {
        false
    }
    fn entities(&self) -> Vec<u64> {
        Vec::new()
    }
    fn has_component<R: ReplicatedComponent>(&self, _world_entity: &u64) -> bool {
        false
    }
    fn has_component_of_kind(&self, _world_entity: &u64, _component_kind: ComponentKind) -> bool {
        false
    }
    fn component<'a, R: ReplicatedComponent>(
        &'a self,
        _entity: &u64,
    ) -> Option<ReplicaRefWrapper<'a, R>> {
        None
    }
    fn component_of_kind<'a>(
        &'a self,
        _entity: &u64,
        _component_kind: ComponentKind,
    ) -> Option<ReplicaDynRefWrapper<'a>> {
        None
    }
}

#[derive(Clone)]
struct OkIdReceiver;

impl IdentityReceiver for OkIdReceiver {
    fn receive(&mut self) -> IdentityReceiverResult {
        IdentityReceiverResult::Success(IdentityToken::generate())
    }
}

/// Fails the first two sends, then succeeds: the attempt must report
/// exactly the failures that preceded its first success.
struct FailTwiceSender {
    attempts: AtomicUsize,
}

impl PacketSender for FailTwiceSender {
    fn send(&self, _payload: &[u8]) -> Result<(), SendError> {
        if self.attempts.fetch_add(1, Ordering::SeqCst) < 2 {
            Err(SendError)
        } else {
            Ok(())
        }
    }
    fn server_addr(&self) -> ServerAddr {
        ServerAddr::Found("127.0.0.1:9999".parse::<SocketAddr>().unwrap())
    }
    fn shutdown(&mut self) {}
}

#[derive(Clone)]
struct QuietReceiver;

impl PacketReceiver for QuietReceiver {
    fn receive(&mut self) -> Result<Option<&[u8]>, RecvError> {
        Ok(None)
    }
    fn server_addr(&self) -> ServerAddr {
        ServerAddr::Found("127.0.0.1:9999".parse::<SocketAddr>().unwrap())
    }
}

#[test]
fn handshake_send_failures_are_counted_until_the_first_success() {
    // Zero send interval so consecutive pump calls each attempt a send
    // without waiting out the production 250 ms spacing.
    let config = ClientConfig {
        send_handshake_interval: Duration::ZERO,
        ..Default::default()
    };
    let mut client = Client::<u64>::new(config, Protocol::builder().build());
    client.io.load(
        Box::new(OkIdReceiver),
        Box::new(FailTwiceSender {
            attempts: AtomicUsize::new(0),
        }),
        Box::new(QuietReceiver),
    );
    client
        .handshake_manager
        .set_identity_token(IdentityToken::generate());

    client.send_all_packets(StubWorld);
    client.send_all_packets(StubWorld);
    assert_eq!(
        client.handshake_failed_sends, 2,
        "the two failed sends must be counted before the first success",
    );
    assert!(
        !client.handshake_first_send_logged,
        "no success yet, so the first-success line must not have fired",
    );

    client.send_all_packets(StubWorld);
    assert!(
        client.handshake_first_send_logged,
        "the first successful send must fire the once-per-attempt line",
    );
    assert_eq!(
        client.handshake_failed_sends, 0,
        "the first success resets the attempt counter",
    );
}
