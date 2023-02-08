use std::any::TypeId;

pub enum EntityAction<E: Copy> {
    SpawnEntity(E, Vec<TypeId>),
    DespawnEntity(E),
    InsertComponent(E, TypeId),
    RemoveComponent(E, TypeId),
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
