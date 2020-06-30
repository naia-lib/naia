use naia_shared::{EventType, LocalEntityKey};

/// An Event that is be emitted by the Client, usually as a result of some communication with the Server
#[derive(Debug)]
pub enum ClientEvent<T: EventType> {
    /// Occurs when the Client has successfully established a connection with the Server
    Connection,
    /// Occurs when the Client has lost connection with the Server, usually as a result of a timeout
    Disconnection,
    /// An Event emitted to the Client from the Server
    Event(T),
    /// Occurs when an Entity on the Server has come into scope for the Client
    CreateEntity(LocalEntityKey),
    /// Occurs when an Entity has had a state change on the Server while in scope for the Client
    UpdateEntity(LocalEntityKey),
    /// Occurs when an Entity on the Server has left the Client's scope
    DeleteEntity(LocalEntityKey),
    /// The Client has no new event from the Server
    None,
}
