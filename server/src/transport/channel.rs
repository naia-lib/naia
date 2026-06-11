use std::net::SocketAddr;

use smol::{
    channel,
    channel::{Receiver, Sender, TryRecvError, TrySendError},
};

use super::{
    PacketReadiness, PacketReceiver as TransportReceiver, PacketSender as TransportSender,
    RecvError, SendError,
};

/// In-process MPSC channel bridging a `PacketSender` / `PacketReceiver` pair for local transport.
pub struct PacketChannel;

impl PacketChannel {
    /// Creates an unbounded in-process channel and returns the sender/receiver pair.
    ///
    /// Alongside the data channel, a coalescing `bounded(1)` readiness
    /// channel is created: the sender pings it on every `send`, and the
    /// receiver exposes it via [`TransportReceiver::readiness`] so the
    /// pipeline recv worker can block event-driven instead of polling.
    pub fn unbounded() -> (Box<dyn TransportSender>, Box<dyn TransportReceiver>) {
        let (data_tx, data_rx) = channel::unbounded();
        // bounded(1) ⇒ at most one buffered "come look" token: a burst of
        // N packets coalesces into a single wake rather than N.
        let (ready_tx, ready_rx) = channel::bounded(1);
        let packet_sender = PacketChannelSender { data_tx, ready_tx };
        let packet_receiver = PacketChannelReceiver::new(data_rx, ready_rx);
        (Box::new(packet_sender), Box::new(packet_receiver))
    }
}

/// In-process sender that enqueues the packet and pings the coalescing
/// readiness channel so a parked recv worker wakes immediately.
#[derive(Clone)]
struct PacketChannelSender {
    data_tx: Sender<(SocketAddr, Box<[u8]>)>,
    ready_tx: Sender<()>,
}

impl TransportSender for PacketChannelSender {
    fn send(&self, address: &SocketAddr, payload: &[u8]) -> Result<(), SendError> {
        self.data_tx
            .send_blocking((*address, payload.into()))
            .map_err(|_| SendError)?;
        // Coalescing ping: ignore `Full` (a token is already buffered, so
        // the worker will wake and drain everything) and `Closed` (the
        // worker is gone; the data send above already succeeded/failed).
        match self.ready_tx.try_send(()) {
            Ok(()) | Err(TrySendError::Full(())) | Err(TrySendError::Closed(())) => {}
        }
        Ok(())
    }
}

#[derive(Clone)]
struct PacketChannelReceiver {
    receiver: Receiver<(SocketAddr, Box<[u8]>)>,
    ready_rx: Receiver<()>,
    current_payload: Option<Box<[u8]>>,
}

impl PacketChannelReceiver {
    fn new(receiver: Receiver<(SocketAddr, Box<[u8]>)>, ready_rx: Receiver<()>) -> Self {
        Self {
            receiver,
            ready_rx,
            current_payload: None,
        }
    }
}

impl TransportReceiver for PacketChannelReceiver {
    /// Receives a packet from the Server Socket
    fn receive(&mut self) -> Result<Option<(SocketAddr, &[u8])>, RecvError> {
        match self.receiver.try_recv() {
            Ok((address, payload)) => {
                self.current_payload = Some(payload);
                Ok(Some((address, self.current_payload.as_ref().unwrap())))
            }
            Err(TryRecvError::Empty) => Ok(None),
            Err(_) => Err(RecvError),
        }
    }

    fn readiness(&self) -> Option<PacketReadiness> {
        Some(PacketReadiness::new(self.ready_rx.clone()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        net::{IpAddr, Ipv4Addr, SocketAddr},
        time::Duration,
    };

    fn addr() -> SocketAddr {
        SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 4000)
    }

    /// `wait()` resolves true iff the readiness fired within the deadline.
    fn woke_within(readiness: &PacketReadiness, ms: u64) -> bool {
        smol::block_on(smol::future::or(
            async {
                readiness.wait().await;
                true
            },
            async {
                smol::Timer::after(Duration::from_millis(ms)).await;
                false
            },
        ))
    }

    #[test]
    fn channel_receiver_reports_readiness() {
        let (_tx, rx) = PacketChannel::unbounded();
        assert!(
            rx.readiness().is_some(),
            "in-process channel must expose readiness"
        );
    }

    #[test]
    fn send_pings_readiness_and_packet_drains() {
        let (tx, mut rx) = PacketChannel::unbounded();
        let readiness = rx.readiness().unwrap();

        // Empty channel: readiness must NOT fire (event-driven, not spurious).
        assert!(
            !woke_within(&readiness, 50),
            "readiness fired with no packet"
        );

        tx.send(&addr(), &[1, 2, 3]).unwrap();
        assert!(
            woke_within(&readiness, 500),
            "readiness did not fire after send"
        );

        match rx.receive().unwrap() {
            Some((a, p)) => {
                assert_eq!(a, addr());
                assert_eq!(p, &[1, 2, 3]);
            }
            None => panic!("expected the sent packet"),
        }
    }

    #[test]
    fn readiness_coalesces_burst_but_all_packets_drain() {
        let (tx, mut rx) = PacketChannel::unbounded();
        let readiness = rx.readiness().unwrap();

        for _ in 0..5 {
            tx.send(&addr(), &[0]).unwrap();
        }

        // A burst collapses to a single buffered token; drain clears it.
        assert!(woke_within(&readiness, 500), "burst did not fire readiness");
        readiness.drain();
        assert!(
            !woke_within(&readiness, 50),
            "stale readiness tokens after drain"
        );

        // ...but every packet is still drainable from the data channel.
        let mut n = 0;
        while rx.receive().unwrap().is_some() {
            n += 1;
        }
        assert_eq!(n, 5, "all burst packets must survive coalescing");
    }
}
