use naia_serde::SerdeInternal;

use crate::handshake::RejectReason;
use crate::ProtocolId;

/// Discriminated-union header prepended to every simple-handshake packet.
#[derive(SerdeInternal, Debug, PartialEq, Eq, Clone)]
pub enum HandshakeHeader {
    /// An initial handshake message sent by the Client to the Server.
    ClientIdentifyRequest(ProtocolId),
    /// The Server's response to the Client's initial handshake message.
    ServerIdentifyResponse,
    /// The handshake message sent by the Client to initiate a connection.
    ClientConnectRequest,
    /// The handshake message sent by the Server indicating the connection has been established.
    ServerConnectResponse,
    /// The Server's rejection response to the Client's connect request.
    ServerRejectResponse(RejectReason),
    /// Used to request a graceful Client disconnect from the Server.
    Disconnect,
}
