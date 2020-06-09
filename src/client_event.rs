use gaia_shared::{EventType, EntityType, LocalEntityKey};

pub enum ClientEvent<T: EventType, U: EntityType> {
    Connection,
    Disconnection,
    Event(T),
    CreateEntity(LocalEntityKey, U),
    UpdateEntity(LocalEntityKey),
    DeleteEntity(LocalEntityKey),
    None,
}