use naia_serde::SerdeInternal;

#[derive(SerdeInternal, Debug, PartialEq, Eq, Clone)]
pub enum HandshakeHeader {
    ClientConnectRequest,
    // The handshake message sent by the Server, indicating that the
    // connection has been established
    ServerConnectResponse,
    // Used to request a graceful Client disconnect from the Server
    Disconnect,
}