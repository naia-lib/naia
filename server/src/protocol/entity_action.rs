use naia_shared::{EntityActionType, ProtocolKindType};

use crate::protocol::entity_manager::ActionId;

#[derive(Clone, PartialEq, Eq)]
pub enum EntityAction<E: Copy, K: ProtocolKindType> {
    SpawnEntity(E),
    DespawnEntity(E),
    InsertComponent(E, K),
    RemoveComponent(E, K),
}

impl<E: Copy, K: ProtocolKindType> EntityAction<E, K> {
    pub fn as_type(&self) -> EntityActionType {
        match self {
            EntityAction::SpawnEntity(_) => EntityActionType::SpawnEntity,
            EntityAction::DespawnEntity(_) => EntityActionType::DespawnEntity,
            EntityAction::InsertComponent(_, _) => EntityActionType::InsertComponent,
            EntityAction::RemoveComponent(_, _) => EntityActionType::RemoveComponent,
        }
    }
}
