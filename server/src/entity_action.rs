use naia_shared::{
    DiffMask, EntityActionType, EntityType, LocalComponentKey, LocalEntity, ProtocolType, Ref,
};

use super::keys::ComponentKey;

#[derive(Clone, Debug)]
pub enum EntityAction<P: ProtocolType, K: EntityType> {
    SpawnEntity(
        K,
        LocalEntity,
        Vec<(ComponentKey, LocalComponentKey, P)>,
    ),
    DespawnEntity(K, LocalEntity),
    OwnEntity(K, LocalEntity),
    DisownEntity(K, LocalEntity),
    InsertComponent(
        LocalEntity,
        ComponentKey,
        LocalComponentKey,
        P,
    ),
    UpdateComponent(
        ComponentKey,
        LocalComponentKey,
        Ref<DiffMask>,
        P,
    ),
    RemoveComponent(ComponentKey, LocalComponentKey),
}

impl<P: ProtocolType, K: EntityType> EntityAction<P, K> {
    pub fn as_type(&self) -> EntityActionType {
        match self {
            EntityAction::SpawnEntity(_, _, _) => EntityActionType::SpawnEntity,
            EntityAction::DespawnEntity(_, _) => EntityActionType::DespawnEntity,
            EntityAction::OwnEntity(_, _) => EntityActionType::OwnEntity,
            EntityAction::DisownEntity(_, _) => EntityActionType::DisownEntity,
            EntityAction::InsertComponent(_, _, _, _) => EntityActionType::InsertComponent,
            EntityAction::UpdateComponent(_, _, _, _) => EntityActionType::UpdateComponent,
            EntityAction::RemoveComponent(_, _) => EntityActionType::RemoveComponent,
        }
    }
}
