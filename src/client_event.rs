
pub enum ClientEvent<T> {
    Connection,
    Disconnection,
    Event(T),
    CreateEntity(u16),
    UpdateEntity(u16),
    DeleteEntity(u16),
    None,
}