use std::{
    net::SocketAddr,
    sync::{Arc, Mutex},
};

use tokio::sync::oneshot::{error::TryRecvError, Receiver};

use crate::server_addr::ServerAddr;

// MaybeAddr
struct MaybeAddr {
    addr: ServerAddr,
    // Dropped once it has yielded an address, so `get` stops polling a channel
    // that can never produce a second one.
    receiver: Option<Receiver<SocketAddr>>,
}

/// Tracks the Server's data-channel address, which is not known until the
/// signaling handshake has produced the Server's ICE candidate.
///
/// `webrtc-unreliable-client` reports that address exactly once, over a
/// oneshot channel. This is the polled view over it that the rest of the
/// socket wants: `get` answers `Finding` until the address arrives and
/// `Found` forever after.
#[derive(Clone)]
pub struct AddrCell {
    cell: Arc<Mutex<MaybeAddr>>,
}

impl AddrCell {
    pub fn new(receiver: Receiver<SocketAddr>) -> Self {
        Self {
            cell: Arc::new(Mutex::new(MaybeAddr {
                addr: ServerAddr::Finding,
                receiver: Some(receiver),
            })),
        }
    }

    pub fn get(&self) -> ServerAddr {
        // A contended lock means another caller is mid-update; reporting
        // `Finding` matches what this would have answered a moment earlier and
        // keeps `get` non-blocking for the send/receive paths that call it.
        let Ok(mut cell) = self.cell.try_lock() else {
            return ServerAddr::Finding;
        };

        if let ServerAddr::Found(addr) = cell.addr {
            return ServerAddr::Found(addr);
        }

        let Some(receiver) = cell.receiver.as_mut() else {
            return ServerAddr::Finding;
        };

        match receiver.try_recv() {
            Ok(addr) => {
                cell.addr = ServerAddr::Found(addr);
                cell.receiver = None;
                ServerAddr::Found(addr)
            }
            Err(TryRecvError::Empty) => ServerAddr::Finding,
            // The socket task ended without resolving an address. Nothing more
            // is coming, so stop polling.
            Err(TryRecvError::Closed) => {
                cell.receiver = None;
                ServerAddr::Finding
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::sync::oneshot;

    fn addr() -> SocketAddr {
        "127.0.0.1:4433".parse().unwrap()
    }

    #[test]
    fn reports_finding_until_the_address_arrives_then_found_forever() {
        let (sender, receiver) = oneshot::channel();
        let cell = AddrCell::new(receiver);

        assert_eq!(cell.get(), ServerAddr::Finding, "nothing sent yet");

        sender.send(addr()).unwrap();

        assert_eq!(cell.get(), ServerAddr::Found(addr()));
        // The oneshot is spent after the first read; the cached value has to
        // survive, or the sender and receiver would disagree about the address
        // depending on who polled first.
        assert_eq!(cell.get(), ServerAddr::Found(addr()));
    }

    #[test]
    fn a_clone_sees_the_address_the_original_resolved() {
        let (sender, receiver) = oneshot::channel();
        let cell = AddrCell::new(receiver);
        let clone = cell.clone();

        sender.send(addr()).unwrap();

        // PacketSender and PacketReceiver each hold a clone, and only one of
        // them will win the race to drain the channel.
        assert_eq!(cell.get(), ServerAddr::Found(addr()));
        assert_eq!(clone.get(), ServerAddr::Found(addr()));
    }

    #[test]
    fn reports_finding_when_the_socket_task_ends_without_an_address() {
        let (sender, receiver) = oneshot::channel::<SocketAddr>();
        let cell = AddrCell::new(receiver);

        drop(sender);

        assert_eq!(cell.get(), ServerAddr::Finding);
        assert_eq!(cell.get(), ServerAddr::Finding, "and stays that way");
    }
}
