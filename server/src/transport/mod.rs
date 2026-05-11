cfg_if! {
    if #[cfg(feature = "transport_webrtc")] {
        pub mod webrtc;
    } else {}
}
cfg_if! {
    if #[cfg(feature = "transport_udp")] {
        pub mod udp;
    } else {}
}
cfg_if! {
    if #[cfg(feature = "transport_local")] {
        pub mod local;
    } else {}
}


mod conditioner;
pub use conditioner::ConditionedPacketReceiver;

mod channel;
pub use channel::PacketChannel;

pub use inner::{
    AuthReceiver, AuthSender, PacketReceiver, PacketSender, RecvError, SendError, Socket,
};
mod inner {

    use std::net::SocketAddr;

    use naia_shared::IdentityToken;

    #[derive(Debug)]
    pub struct SendError;

    /// Transport-layer receive failure signal. Carries no payload because the
    /// underlying OS error is already logged at the transport site before this
    /// error is returned. All recovery paths are identical: wait for the
    /// connection-timeout disconnect event.
    #[derive(Debug)]
    pub struct RecvError;

    pub trait Socket {
        fn listen(
            self: Box<Self>,
        ) -> (
            Box<dyn AuthSender>,
            Box<dyn AuthReceiver>,
            Box<dyn PacketSender>,
            Box<dyn PacketReceiver>,
        );
    }

    // Packet

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

    pub trait AuthSender: Send + Sync {
        ///
        fn accept(
            &self,
            address: &SocketAddr,
            identity_token: &IdentityToken,
        ) -> Result<(), SendError>;
        ///
        fn reject(&self, address: &SocketAddr) -> Result<(), SendError>;
    }

    pub trait AuthReceiver: AuthReceiverClone + Send + Sync {
        ///
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
