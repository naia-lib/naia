use std::net::SocketAddr;

pub enum ServerEvent {
    Connection(SocketAddr),
    Disconnection(SocketAddr),
    Message(SocketAddr, String),
    Tick,
}