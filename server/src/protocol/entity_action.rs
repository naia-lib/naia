use naia_shared::{EntityActionType, ProtocolKindType};

#[derive(Clone, PartialEq, Eq)]
pub enum EntityAction<K: ProtocolKindType, E: Copy> {
    SpawnEntity(E),
    DespawnEntity(E),
    InsertComponent(E, K),
    RemoveComponent(E, K),
    Noop,
}

impl<K: ProtocolKindType, E: Copy> EntityAction<K, E> {
    pub fn as_type(&self) -> EntityActionType {
        match self {
            EntityAction::SpawnEntity(_) => EntityActionType::SpawnEntity,
            EntityAction::DespawnEntity(_) => EntityActionType::DespawnEntity,
            EntityAction::InsertComponent(_, _) => EntityActionType::InsertComponent,
            EntityAction::RemoveComponent(_, _) => EntityActionType::RemoveComponent,
            EntityAction::Noop => EntityActionType::Noop,
        }
    }
}

pub enum EntityActionRecord<K: ProtocolKindType, E: Copy> {
    SpawnEntity(E, Vec<K>),
    DespawnEntity(E),
    InsertComponent(E, K),
    RemoveComponent(E, K),
    Noop,
}
