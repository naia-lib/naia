use naia_shared::{DiffMask, EntityActionType, LocalComponentKey, LocalEntity, ProtocolType};

use super::keys::ComponentKey;

#[derive(Clone, Debug)]
pub enum EntityAction<P: ProtocolType, E: Copy> {
    SpawnEntity(
        E,
        LocalEntity,
        Vec<(ComponentKey, LocalComponentKey, P::Kind)>,
    ),
    DespawnEntity(E, LocalEntity),
    OwnEntity(E, LocalEntity),
    DisownEntity(E, LocalEntity),
    InsertComponent(E, LocalEntity, ComponentKey, LocalComponentKey, P::Kind),
    UpdateComponent(E, ComponentKey, LocalComponentKey, DiffMask, P::Kind),
    RemoveComponent(ComponentKey, LocalComponentKey),
}

impl<P: ProtocolType, E: Copy> EntityAction<P, E> {
    pub fn as_type(&self) -> EntityActionType {
        match self {
            EntityAction::SpawnEntity(_, _, _) => EntityActionType::SpawnEntity,
            EntityAction::DespawnEntity(_, _) => EntityActionType::DespawnEntity,
            EntityAction::OwnEntity(_, _) => EntityActionType::OwnEntity,
            EntityAction::DisownEntity(_, _) => EntityActionType::DisownEntity,
            EntityAction::InsertComponent(_, _, _, _, _) => EntityActionType::InsertComponent,
            EntityAction::UpdateComponent(_, _, _, _, _) => EntityActionType::UpdateComponent,
            EntityAction::RemoveComponent(_, _) => EntityActionType::RemoveComponent,
        }
    }
}
