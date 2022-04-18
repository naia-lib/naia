use naia_shared::ProtocolKindType;

#[derive(Clone, PartialEq, Eq)]
pub enum EntityActionEvent<E: Copy, K: ProtocolKindType> {
    SpawnEntity(E),
    DespawnEntity(E),
    InsertComponent(E, K),
    RemoveComponent(E, K),
}
