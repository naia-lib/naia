
pub enum ClientEvent<T> {
    Connection,
    Disconnection,
    Event(T),
    CreateEntity,
    UpdateEntity,
    DeletEntity,
    None,
}