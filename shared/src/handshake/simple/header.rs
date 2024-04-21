use naia_serde::SerdeInternal;

#[derive(SerdeInternal, Debug, PartialEq, Eq, Clone)]
pub enum HandshakeHeader {
    // An initial handshake message sent by the Client to the Server
    ClientIdentifyRequest,
    // The Server's response to the Client's initial handshake message
    ServerIdentifyResponse,
    // The handshake message sent by the Client to initiate a connection
    ClientConnectRequest,
    // The handshake message sent by the Server, indicating that the
    // connection has been established
    ServerConnectResponse,
    // Used to request a graceful Client disconnect from the Server
    Disconnect,
}