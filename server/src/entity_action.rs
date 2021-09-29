use naia_shared::{
    DiffMask, EntityActionType, LocalComponentKey, LocalEntityKey, ProtocolType, Ref, Replicate,
};

use super::keys::{ComponentKey, KeyType};

#[derive(Clone, Debug)]
pub enum EntityAction<P: ProtocolType, K: KeyType> {
    SpawnEntity(
        K,
        LocalEntityKey,
        Vec<(ComponentKey, LocalComponentKey, Ref<dyn Replicate<P>>)>,
    ),
    DespawnEntity(K, LocalEntityKey),
    OwnEntity(K, LocalEntityKey),
    DisownEntity(K, LocalEntityKey),
    InsertComponent(
        LocalEntityKey,
        ComponentKey,
        LocalComponentKey,
        Ref<dyn Replicate<P>>,
    ),
    UpdateComponent(
        ComponentKey,
        LocalComponentKey,
        Ref<DiffMask>,
        Ref<dyn Replicate<P>>,
    ),
    RemoveComponent(ComponentKey, LocalComponentKey),
}

impl<P: ProtocolType, K: KeyType> EntityAction<P, K> {
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
