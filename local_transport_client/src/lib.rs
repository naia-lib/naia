mod client;

// client stuff
pub use local_transport_shared::{
    ClientIdentityReceiverResult, ClientServerAddr
};
pub use client::{LocalClientIdentity, LocalClientSocket, LocalClientSender, LocalClientReceiver, LocalAddrCell};