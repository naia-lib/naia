//! In-memory local transport layers used by `transport_local` feature flag for quick E2E tests.
//!
//! Provides paired client/server sockets with types that match the transport-level trait signatures.
//! The transport layer modules wrap these types to implement the transport traits.

mod runtime;
mod shared;
mod hub;
mod endpoint;
mod builder;
pub mod client;
pub mod server;

#[cfg(test)]
mod tests;

pub use endpoint::{LocalServerEndpoint, LocalClientEndpoint};
pub use builder::LocalTransportBuilder;
pub use shared::{
    ClientIdentityReceiverResult, ClientServerAddr,
    ClientSendError, ClientRecvError, ServerSendError, ServerRecvError,
};

pub use client::{LocalClientSocket, LocalClientSender, LocalClientReceiver, LocalClientIdentity};
pub use server::{LocalServerSocket, LocalServerSender, LocalServerReceiver, LocalServerAuthSender, LocalServerAuthReceiver};
