use super::protocolize::ProtocolKindType;

pub enum EntityAction<E: Copy, K: ProtocolKindType> {
    SpawnEntity(E, Vec<K>),
    DespawnEntity(E),
    InsertComponent(E, K),
    RemoveComponent(E, K),
    Noop,
}

impl<E: Copy, K: ProtocolKindType> EntityAction<E, K> {
    pub fn entity(&self) -> Option<E> {
        match self {
            EntityAction::SpawnEntity(entity, _) => Some(*entity),
            EntityAction::DespawnEntity(entity) => Some(*entity),
            EntityAction::InsertComponent(entity, _) => Some(*entity),
            EntityAction::RemoveComponent(entity, _) => Some(*entity),
            EntityAction::Noop => None,
        }
    }
}
