use naia_shared::{EntityActionType, Protocolize};

#[derive(Clone)]
pub enum EntityAction<P: Protocolize, E: Copy> {
    SpawnEntity(E),
    DespawnEntity(E),
    InsertComponent(E, P::Kind),
    RemoveComponent(E, P::Kind),
    Noop,
}

impl<P: Protocolize, E: Copy> EntityAction<P, E> {
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
