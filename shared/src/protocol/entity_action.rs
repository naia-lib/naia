use super::protocolize::ProtocolKindType;

#[derive(Clone, PartialEq, Eq)]
pub enum EntityAction<E: Copy, K: ProtocolKindType> {
    SpawnEntity(E),
    DespawnEntity(E),
    InsertComponent(E, K),
    RemoveComponent(E, K),
    Noop,
}
