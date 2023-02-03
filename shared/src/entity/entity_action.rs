use crate::types::ComponentId;

pub enum EntityAction<E: Copy> {
    SpawnEntity(E, Vec<ComponentId>),
    DespawnEntity(E),
    InsertComponent(E, ComponentId),
    RemoveComponent(E, ComponentId),
    Noop,
}

impl<E: Copy> EntityAction<E> {
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
