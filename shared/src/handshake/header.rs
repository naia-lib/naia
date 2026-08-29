use naia_serde::SerdeInternal;

use crate::handshake::RejectReason;
use crate::{DisconnectReason, ProtocolId};

/// Discriminated-union header prepended to every handshake packet.
///
/// This is the union of both handshake flows. Builds with `address_validation`
/// begin at the Challenge/Validate pair, which proves the client owns the
/// source address it claims; builds without it begin at the Identify pair.
/// Everything from `ClientConnectRequest` onward is common to both.
///
/// NOTE: `SerdeInternal` encodes this by positional variant index, so a client
/// and a server must be built with the same `address_validation` setting to
/// interoperate.
#[derive(SerdeInternal, Debug, PartialEq, Eq, Clone)]
pub enum HandshakeHeader {
    /// An initial handshake message sent by the Client to the Server.
    ClientChallengeRequest(ProtocolId),
    /// The Server's response to the Client's challenge request.
    ServerChallengeResponse,
    /// The handshake message validating the Client.
    ClientValidateRequest,
    /// The Server's response to the Client's validation request.
    ServerValidateResponse,
    /// An initial handshake message sent by the Client to the Server.
    ClientIdentifyRequest(ProtocolId),
    /// The Server's response to the Client's identify request.
    ServerIdentifyResponse,
    /// The handshake message sent by the Client to initiate a connection.
    ClientConnectRequest,
    /// The handshake message sent by the Server indicating the connection has been established.
    ServerConnectResponse,
    /// The Server's rejection response to the Client's connect request.
    ServerRejectResponse(RejectReason),
    /// Used to request a graceful Client disconnect from the Server.
    Disconnect,
    /// The Server dropping an established Client, saying why.
    ///
    /// Followed on the wire by an `Option<Vec<u8>>`: a serialized message the
    /// client decodes against its own protocol to learn the reason in the
    /// application's own terms (naia-lib/naia#10). The plain `Disconnect`
    /// variant above is the Client's half of the exchange and is unchanged.
    ServerDisconnect(DisconnectReason),
}
