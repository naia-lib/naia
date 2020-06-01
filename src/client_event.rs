
use gaia_shared::NetEvent;

pub enum ClientEvent {
    Connection,
    Disconnection,
    Message(String),
    Event(Box<dyn NetEvent>),
    None,
}