use naia_serde::SerdeInternal;

use crate::ProtocolId;
use crate::handshake::RejectReason;

#[derive(SerdeInternal, Debug, PartialEq, Eq, Clone)]
pub enum HandshakeHeader {
    // An initial handshake message sent by the Client to the Server
    ClientChallengeRequest(ProtocolId),
    // The Server's response to the Client's initial handshake message
    ServerChallengeResponse,
    // The handshake message validating the Client
    ClientValidateRequest,
    // The Server's response to the Client's validation request
    ServerValidateResponse,
    // The final handshake message sent by the Client
    ClientConnectRequest,
    // The final handshake message sent by the Server, indicating that the
    // connection has been established
    ServerConnectResponse,
    // The Server's response to the Client's initial handshake message,
    // indicating that the connection was rejected
    ServerRejectResponse(RejectReason),
    // Used to request a graceful Client disconnect from the Server
    Disconnect,
}
