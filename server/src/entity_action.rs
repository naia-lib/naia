use naia_shared::{DiffMask, EntityActionType, ProtocolType};

use super::keys::ComponentKey;

#[derive(Clone, Debug)]
pub enum EntityAction<P: ProtocolType, E: Copy> {
    SpawnEntity(E, Vec<(ComponentKey, P::Kind)>),
    DespawnEntity(E),
    OwnEntity(E),
    DisownEntity(E),
    InsertComponent(E, ComponentKey, P::Kind),
    UpdateComponent(E, ComponentKey, DiffMask, P::Kind),
    RemoveComponent(ComponentKey),
}

impl<P: ProtocolType, E: Copy> EntityAction<P, E> {
    pub fn as_type(&self) -> EntityActionType {
        match self {
            EntityAction::SpawnEntity(_, _) => EntityActionType::SpawnEntity,
            EntityAction::DespawnEntity(_) => EntityActionType::DespawnEntity,
            EntityAction::OwnEntity(_) => EntityActionType::OwnEntity,
            EntityAction::DisownEntity(_) => EntityActionType::DisownEntity,
            EntityAction::InsertComponent(_, _, _) => EntityActionType::InsertComponent,
            EntityAction::UpdateComponent(_, _, _, _) => EntityActionType::UpdateComponent,
            EntityAction::RemoveComponent(_) => EntityActionType::RemoveComponent,
        }
    }
}
