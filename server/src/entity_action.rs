use naia_shared::{DiffMask, EntityActionType, Protocolize};

use super::keys::ComponentKey;

#[derive(Debug)]
pub enum EntityAction<P: Protocolize, E: Copy> {
    SpawnEntity(E, Vec<(ComponentKey, P::Kind)>),
    DespawnEntity(E),
    MessageEntity(E, P),
    InsertComponent(E, ComponentKey, P::Kind),
    UpdateComponent(E, ComponentKey, DiffMask, P::Kind),
    RemoveComponent(ComponentKey),
}

impl<P: Protocolize, E: Copy> EntityAction<P, E> {
    pub fn as_type(&self) -> EntityActionType {
        match self {
            EntityAction::SpawnEntity(_, _) => EntityActionType::SpawnEntity,
            EntityAction::DespawnEntity(_) => EntityActionType::DespawnEntity,
            EntityAction::MessageEntity(_, _) => EntityActionType::MessageEntity,
            EntityAction::InsertComponent(_, _, _) => EntityActionType::InsertComponent,
            EntityAction::UpdateComponent(_, _, _, _) => EntityActionType::UpdateComponent,
            EntityAction::RemoveComponent(_) => EntityActionType::RemoveComponent,
        }
    }
}

impl<P: Protocolize, E: Copy> Clone for EntityAction<P, E> {
    fn clone(&self) -> Self {
        match self {
            EntityAction::SpawnEntity(a, b) => EntityAction::SpawnEntity(*a, b.clone()),
            EntityAction::DespawnEntity(a) => EntityAction::DespawnEntity(*a),
            EntityAction::MessageEntity(a, b) => EntityAction::MessageEntity(*a, b.clone()),
            EntityAction::InsertComponent(a, b, c) => EntityAction::InsertComponent(*a, *b, *c),
            EntityAction::UpdateComponent(a, b, c, d) => {
                EntityAction::UpdateComponent(*a, *b, c.clone(), *d)
            }
            EntityAction::RemoveComponent(a) => EntityAction::RemoveComponent(*a),
        }
    }
}
