use super::protocolize::ProtocolKindType;

#[derive(Clone, PartialEq, Eq)]
pub enum EntityAction<E: Copy, K: ProtocolKindType> {
    SpawnEntity(E),
    DespawnEntity(E),
    InsertComponent(E, K),
    RemoveComponent(E, K),
    Noop,
}

impl<E: Copy, K: ProtocolKindType> EntityAction<E, K> {
    pub fn entity(&self) -> Option<E> {
        match self {
            EntityAction::SpawnEntity(entity) => Some(*entity),
            EntityAction::DespawnEntity(entity) => Some(*entity),
            EntityAction::InsertComponent(entity, _) => Some(*entity),
            EntityAction::RemoveComponent(entity, _) => Some(*entity),
            EntityAction::Noop => None,
        }
    }
}
