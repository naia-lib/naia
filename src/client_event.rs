
pub enum ClientEvent {
    Connection,
    Disconnection,
    Message(String),
    None,
}