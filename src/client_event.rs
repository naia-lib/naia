use gaia_shared::{EventType, LocalEntityKey};

pub enum ClientEvent<T: EventType> {
    Connection,
    Disconnection,
    Event(T),
    CreateEntity(LocalEntityKey),
    UpdateEntity(LocalEntityKey),
    DeleteEntity(LocalEntityKey),
    None,
}