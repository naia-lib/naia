use std::net::SocketAddr;

pub enum ServerEvent<T> {
    Connection(SocketAddr),
    Disconnection(SocketAddr),
    Event(SocketAddr, T),
    Tick,
}