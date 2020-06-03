
pub enum ClientEvent<T> {
    Connection,
    Disconnection,
    Message(String),
    Event(T),
    None,
}