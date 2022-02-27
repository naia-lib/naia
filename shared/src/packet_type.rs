use crate::{derive_serde, serde};

// An enum representing the different types of packets that can be
// sent/received

#[derive(Copy, Debug)]
#[derive_serde]
pub enum PacketType {
    // A packet containing Message/Entity/Component data
    Data,
    // A packet sent to maintain the connection by preventing a timeout
    Heartbeat,
    // An initial handshake message sent by the Client to the Server
    ClientChallengeRequest,
    // The Server's response to the Client's initial handshake message
    ServerChallengeResponse,
    // The final handshake message sent by the Client
    ClientConnectRequest,
    // The final handshake message sent by the Server, indicating that the
    // connection has been established
    ServerConnectResponse,
    // A Ping message, used to calculate RTT. Must be responded to with a Pong
    // message
    Ping,
    // A Pong message, used to calculate RTT. Must be the response to all Ping
    // messages
    Pong,
    // Used to request a graceful Client disconnect from the Server
    Disconnect,
}
