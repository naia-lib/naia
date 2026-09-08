cfg_if! {
    if #[cfg(feature = "transport_webrtc")] {
        #[doc(hidden)]
        pub mod webrtc;
    } else {}
}
cfg_if! {
    if #[cfg(feature = "transport_udp")] {
        #[doc(hidden)]
        pub mod udp;
    } else {}
}
cfg_if! {
    if #[cfg(feature = "transport_local")] {
        #[doc(hidden)]
        pub mod local;
    } else {}
}

mod conditioner;
pub use conditioner::ConditionedPacketReceiver;

mod channel;
pub use channel::PacketChannel;

pub use inner::{
    AuthReceiver, AuthSender, ListenResult, PacketReadiness, PacketReceiver, PacketSender,
    RecvError, SendError, Socket,
};
mod inner {

    use std::net::SocketAddr;

    use naia_shared::{IdentityToken, ProtocolId};

    /// Tuple returned by [`Socket::listen`]: auth sender, auth receiver, packet sender, packet receiver.
    pub type ListenResult = (
        Box<dyn AuthSender>,
        Box<dyn AuthReceiver>,
        Box<dyn PacketSender>,
        Box<dyn PacketReceiver>,
    );

    /// Error returned when a packet could not be sent to a remote address.
    #[derive(Debug)]
    pub struct SendError;

    /// Transport-layer receive failure signal. Carries no payload because the
    /// underlying OS error is already logged at the transport site before this
    /// error is returned. All recovery paths are identical: wait for the
    /// connection-timeout disconnect event.
    #[derive(Debug)]
    pub struct RecvError;

    /// Entry point for a server transport: converts the socket into its four I/O handles.
    pub trait Socket {
        /// Binds / starts listening and returns the four I/O channel handles.
        ///
        /// `expected_protocol_id` is this server's protocol fingerprint. Every
        /// transport that accepts auth envelopes must compare the fingerprint
        /// the peer sent against it and drop the request if they differ,
        /// *before* the credential is base64-decoded and before anything is
        /// handed back through [`AuthReceiver`]. Taking it here rather than
        /// through a setter is what makes that impossible to forget: a
        /// transport cannot be listening without having been told what to
        /// compare against.
        fn listen(self: Box<Self>, expected_protocol_id: ProtocolId) -> ListenResult;
    }

    // Packet

    /// Sends raw UDP/WebRTC packets from the server to a remote client address.
    pub trait PacketSender: PacketSenderClone + Send + Sync {
        /// Sends a packet to the Server Socket
        fn send(&self, address: &SocketAddr, payload: &[u8]) -> Result<(), SendError>;
    }

    /// Used to clone Box<dyn PacketSender>
    pub trait PacketSenderClone {
        /// Clone the boxed PacketSender
        fn clone_box(&self) -> Box<dyn PacketSender>;
    }

    impl<T: 'static + PacketSender + Clone> PacketSenderClone for T {
        fn clone_box(&self) -> Box<dyn PacketSender> {
            Box::new(self.clone())
        }
    }

    impl Clone for Box<dyn PacketSender> {
        fn clone(&self) -> Box<dyn PacketSender> {
            PacketSenderClone::clone_box(self.as_ref())
        }
    }

    /// Awaitable readiness signal for event-driven transports.
    ///
    /// Wraps a coalescing `bounded(1)` channel that the matching sender
    /// pings on every `send`. A consumer (the pipeline recv worker) can
    /// `wait().await` to block with zero CPU until a packet *may* be
    /// available, instead of polling `receive()` on a timer. Cloneable so
    /// the worker can hold its own handle.
    #[derive(Clone)]
    pub struct PacketReadiness(smol::channel::Receiver<()>);

    impl PacketReadiness {
        /// Construct from the receiving half of a sender-pinged `()` channel.
        pub fn new(rx: smol::channel::Receiver<()>) -> Self {
            Self(rx)
        }

        /// Resolve when the transport may have data (or the signal channel
        /// closed — e.g. the sender was dropped on shutdown). The caller
        /// must then drain via `receive()` and re-check its own
        /// park/shutdown state; a spurious wake is harmless.
        pub async fn wait(&self) {
            let _ = self.0.recv().await;
        }

        /// Clear any buffered readiness tokens so a burst of packets that
        /// produced multiple pings collapses into a single wake (the
        /// coalescing `bounded(1)` buffer means at most one is ever held,
        /// but draining keeps the contract explicit).
        pub fn drain(&self) {
            while self.0.try_recv().is_ok() {}
        }
    }

    /// Polls for the next incoming packet from any connected client.
    pub trait PacketReceiver: PacketReceiverClone + Send + Sync {
        /// Receives a packet from the Server Socket
        fn receive(&mut self) -> Result<Option<(SocketAddr, &[u8])>, RecvError>;

        /// Awaitable readiness, for transports that can signal it cheaply.
        ///
        /// `Some` ⇒ this transport pings a readiness channel on every
        /// inbound packet (in-process [`PacketChannel`]); a consumer may
        /// block on [`PacketReadiness::wait`] instead of polling
        /// `receive()`. `None` (the default) ⇒ poll-only — raw blocking
        /// socket transports (UDP/WebRTC) have no awaitable readiness
        /// without async socket I/O, so their consumers must keep polling.
        fn readiness(&self) -> Option<PacketReadiness> {
            None
        }
    }

    /// Used to clone Box<dyn PacketReceiver>
    pub trait PacketReceiverClone {
        /// Clone the boxed PacketReceiver
        fn clone_box(&self) -> Box<dyn PacketReceiver>;
    }

    impl<T: 'static + PacketReceiver + Clone> PacketReceiverClone for T {
        fn clone_box(&self) -> Box<dyn PacketReceiver> {
            Box::new(self.clone())
        }
    }

    impl Clone for Box<dyn PacketReceiver> {
        fn clone(&self) -> Box<dyn PacketReceiver> {
            PacketReceiverClone::clone_box(self.as_ref())
        }
    }

    // Auth

    /// Accepts or rejects pending client authentication requests.
    pub trait AuthSender: Send + Sync {
        /// Accept a client's auth request and issue the given identity token.
        fn accept(
            &self,
            address: &SocketAddr,
            identity_token: &IdentityToken,
        ) -> Result<(), SendError>;
        /// Reject a client's auth request, causing the client to disconnect.
        ///
        /// `payload`, when present, is an already-serialized message the client
        /// can decode to learn *why* it was rejected. It rides in the body of
        /// the rejection response, so it must survive a transport that carries
        /// text: implementations base64-encode it.
        fn reject(&self, address: &SocketAddr, payload: Option<&[u8]>) -> Result<(), SendError>;
    }

    /// Receives raw auth payloads from connecting clients before they are handed the session.
    ///
    /// # Protocol-fingerprint guarantee
    ///
    /// Nothing reaches this trait until the peer's protocol fingerprint has
    /// been compared against the one passed to [`Socket::listen`] and found
    /// equal. A request whose fingerprint is absent, malformed, the wrong
    /// width, or simply different is dropped inside the transport, on one
    /// branch, before its credential is decoded. Callers may therefore treat
    /// every payload they get here as coming from a peer that agrees on the
    /// protocol — but must still treat the payload itself as untrusted.
    pub trait AuthReceiver: AuthReceiverClone + Send + Sync {
        /// Poll for the next pending auth payload, returning `Ok(None)` when none are queued.
        fn receive(&mut self) -> Result<Option<(SocketAddr, &[u8])>, RecvError>;
    }

    /// Used to clone Box<dyn AuthReceiver>
    pub trait AuthReceiverClone {
        /// Clone the boxed AuthReceiver
        fn clone_box(&self) -> Box<dyn AuthReceiver>;
    }

    impl<T: 'static + AuthReceiver + Clone> AuthReceiverClone for T {
        fn clone_box(&self) -> Box<dyn AuthReceiver> {
            Box::new(self.clone())
        }
    }

    impl Clone for Box<dyn AuthReceiver> {
        fn clone(&self) -> Box<dyn AuthReceiver> {
            AuthReceiverClone::clone_box(self.as_ref())
        }
    }
}
