cfg_if! {
    if #[cfg(feature = "transport_webrtc")] {
        #[doc(hidden)]
        pub mod webrtc;
    }
}
cfg_if! {
    if #[cfg(feature = "transport_udp")] {
        #[doc(hidden)]
        pub mod udp;
    }
}
cfg_if! {
    if #[cfg(feature = "transport_local")] {
        #[doc(hidden)]
        pub mod local;
    }
}

mod conditioner;
pub use conditioner::ConditionedPacketReceiver;

mod server_addr;

pub use server_addr::ServerAddr;

pub use inner::{
    IdentityReceiver, IdentityReceiverResult, PacketReceiver, PacketSender, RecvError, SendError,
    Socket,
};

mod inner {

    use naia_shared::IdentityToken;
    /// Result of polling the identity handshake for an authentication token from the server.
    pub enum IdentityReceiverResult {
        /// The server has not yet replied; poll again next frame.
        Waiting,
        /// The server accepted the connection and returned an identity token.
        Success(IdentityToken),
        /// The server rejected the connection with an HTTP-style error code.
        ErrorResponseCode(u16),
    }

    use super::ServerAddr;

    /// Transport-layer send failure signal; the caller should treat the connection as broken.
    pub struct SendError;

    /// Transport-layer receive failure signal. Carries no payload because the
    /// underlying OS error is already logged at the transport site before this
    /// error is returned. All recovery paths are identical: wait for the
    /// connection-timeout disconnect event.
    pub struct RecvError;

    /// Transport-layer socket factory; consumed on connect to produce sender and receiver halves.
    pub trait Socket {
        /// Connects without authentication, returning identity, sender, and receiver handles.
        fn connect(
            self: Box<Self>,
        ) -> (
            Box<dyn IdentityReceiver>,
            Box<dyn PacketSender>,
            Box<dyn PacketReceiver>,
        );
        /// Connects with raw auth bytes embedded in the handshake.
        fn connect_with_auth(
            self: Box<Self>,
            auth_bytes: Vec<u8>,
        ) -> (
            Box<dyn IdentityReceiver>,
            Box<dyn PacketSender>,
            Box<dyn PacketReceiver>,
        );
        /// Connects with HTTP-style auth headers added to the upgrade request.
        fn connect_with_auth_headers(
            self: Box<Self>,
            auth_headers: Vec<(String, String)>,
        ) -> (
            Box<dyn IdentityReceiver>,
            Box<dyn PacketSender>,
            Box<dyn PacketReceiver>,
        );
        /// Connects with both raw auth bytes and HTTP-style auth headers.
        fn connect_with_auth_and_headers(
            self: Box<Self>,
            auth_bytes: Vec<u8>,
            auth_headers: Vec<(String, String)>,
        ) -> (
            Box<dyn IdentityReceiver>,
            Box<dyn PacketSender>,
            Box<dyn PacketReceiver>,
        );
    }

    /// Owned, thread-safe handle for sending raw packets to the server.
    pub trait PacketSender: Send + Sync {
        /// Sends a packet from the Client Socket
        fn send(&self, payload: &[u8]) -> Result<(), SendError>;
        /// Get the Server's Socket address
        fn server_addr(&self) -> ServerAddr;
    }

    /// Owned, cloneable, thread-safe handle for polling raw packets from the server.
    pub trait PacketReceiver: PacketReceiverClone + Send + Sync {
        /// Receives a packet from the Client Socket
        fn receive(&mut self) -> Result<Option<&[u8]>, RecvError>;
        /// Get the Server's Socket address
        fn server_addr(&self) -> ServerAddr;
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

    // Identity

    /// Polls the server for the identity token that completes the handshake.
    pub trait IdentityReceiver: IdentityReceiverClone + Send + Sync {
        /// Polls the server for the identity token; returns [`IdentityReceiverResult::Waiting`] until the handshake completes.
        fn receive(&mut self) -> IdentityReceiverResult;
    }

    /// Used to clone Box<dyn IdentityReceiver>
    pub trait IdentityReceiverClone {
        /// Clone the boxed IdentityReceiver
        fn clone_box(&self) -> Box<dyn IdentityReceiver>;
    }

    impl<T: 'static + IdentityReceiver + Clone> IdentityReceiverClone for T {
        fn clone_box(&self) -> Box<dyn IdentityReceiver> {
            Box::new(self.clone())
        }
    }

    impl Clone for Box<dyn IdentityReceiver> {
        fn clone(&self) -> Box<dyn IdentityReceiver> {
            IdentityReceiverClone::clone_box(self.as_ref())
        }
    }
}
