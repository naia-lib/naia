use naia_serde::SerdeInternal;

/// Reason a server-side rejection occurred during the handshake.
#[derive(SerdeInternal, Debug, PartialEq, Eq, Clone, Copy)]
pub enum RejectReason {
    /// The client's protocol ID did not match the server's compiled protocol.
    ProtocolMismatch,
    /// The server application explicitly rejected the client's auth message.
    Auth,
}
