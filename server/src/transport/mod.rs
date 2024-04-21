cfg_if! {
    if #[cfg(feature = "transport_webrtc")] {
        pub mod webrtc;
    } else {}
}
cfg_if! {
    if #[cfg(feature = "transport_udp")] {
        pub mod udp;
        mod conditioner;
    } else {}
}

pub use inner::{PacketReceiver, PacketSender, AuthSender, AuthReceiver, RecvError, SendError, Socket};

mod inner {

    use std::net::SocketAddr;
    use naia_shared::IdentityToken;

    pub struct SendError;

    pub struct RecvError;

    pub trait Socket {
        fn listen(self: Box<Self>) -> (Box<dyn AuthSender>, Box<dyn AuthReceiver>, Box<dyn PacketSender>, Box<dyn PacketReceiver>);
    }

    // Packet

    pub trait PacketSender: Send + Sync {
        /// Sends a packet to the Server Socket
        fn send(&self, address: &SocketAddr, payload: &[u8]) -> Result<(), SendError>;
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
        fn accept(&self, address: &SocketAddr, identity_token: &IdentityToken) -> Result<(), SendError>;
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
