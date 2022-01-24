use naia_shared::{DiffMask, EntityActionType, ProtocolType};

use super::keys::ComponentKey;

#[derive(Debug)]
pub enum EntityAction<P: ProtocolType, E: Copy> {
    SpawnEntity(E, Vec<(ComponentKey, P::Kind)>),
    DespawnEntity(E),
    InsertComponent(E, ComponentKey, P::Kind),
    UpdateComponent(E, ComponentKey, DiffMask, P::Kind),
    RemoveComponent(ComponentKey),
}

impl<P: ProtocolType, E: Copy> EntityAction<P, E> {
    pub fn as_type(&self) -> EntityActionType {
        match self {
            EntityAction::SpawnEntity(_, _) => EntityActionType::SpawnEntity,
            EntityAction::DespawnEntity(_) => EntityActionType::DespawnEntity,
            EntityAction::InsertComponent(_, _, _) => EntityActionType::InsertComponent,
            EntityAction::UpdateComponent(_, _, _, _) => EntityActionType::UpdateComponent,
            EntityAction::RemoveComponent(_) => EntityActionType::RemoveComponent,
        }
    }
}

impl<P: ProtocolType, E: Copy> Clone for EntityAction<P, E> {
    fn clone(&self) -> Self {
        match self {
            EntityAction::SpawnEntity(a, b) => EntityAction::SpawnEntity(*a, b.clone()),
            EntityAction::DespawnEntity(a) => EntityAction::DespawnEntity(*a),
            EntityAction::InsertComponent(a, b, c) => EntityAction::InsertComponent(*a, *b, *c),
            EntityAction::UpdateComponent(a, b, c, d) => {
                EntityAction::UpdateComponent(*a, *b, c.clone(), *d)
            }
            EntityAction::RemoveComponent(a) => EntityAction::RemoveComponent(*a),
        }
    }
}
