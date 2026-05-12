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
    AuthReceiver, AuthSender, ListenResult, PacketReceiver, PacketSender, RecvError, SendError,
    Socket,
};
mod inner {

    use std::net::SocketAddr;

    use naia_shared::IdentityToken;

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
        fn listen(self: Box<Self>) -> ListenResult;
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

    /// Polls for the next incoming packet from any connected client.
    pub trait PacketReceiver: PacketReceiverClone + Send + Sync {
        /// Receives a packet from the Server Socket
        fn receive(&mut self) -> Result<Option<(SocketAddr, &[u8])>, RecvError>;
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
        fn reject(&self, address: &SocketAddr) -> Result<(), SendError>;
    }

    /// Receives raw auth payloads from connecting clients before they are handed the session.
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
