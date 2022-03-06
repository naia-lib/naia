use naia_shared::{EntityActionType, Protocolize};

#[derive(Clone)]
pub enum EntityAction<P: Protocolize, E: Copy> {
    SpawnEntity(E, Option<Vec<P::Kind>>),
    DespawnEntity(E),
    MessageEntity(P),
    InsertComponent(E, P::Kind),
    UpdateComponent(E, P::Kind),
    RemoveComponent(E, P::Kind),
}

impl<P: Protocolize, E: Copy> EntityAction<P, E> {
    pub fn as_type(&self) -> EntityActionType {
        match self {
            EntityAction::SpawnEntity { .. } => EntityActionType::SpawnEntity,
            EntityAction::DespawnEntity(_) => EntityActionType::DespawnEntity,
            EntityAction::MessageEntity(_) => EntityActionType::MessageEntity,
            EntityAction::InsertComponent(_, _) => EntityActionType::InsertComponent,
            EntityAction::UpdateComponent(_, _) => EntityActionType::UpdateComponent,
            EntityAction::RemoveComponent(_, _) => EntityActionType::RemoveComponent,
        }
    }
}
