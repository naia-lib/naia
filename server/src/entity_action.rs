use naia_shared::{
    DiffMask, EntityActionType, EntityKey, LocalComponentKey, LocalEntityKey, ProtocolType, Ref,
    Replicate,
};

use super::keys::component_key::ComponentKey;

#[derive(Debug)]
pub enum EntityAction<P: ProtocolType> {
    SpawnEntity(
        EntityKey,
        LocalEntityKey,
        Option<Vec<(ComponentKey, LocalComponentKey, Ref<dyn Replicate<P>>)>>,
    ),
    DespawnEntity(EntityKey, LocalEntityKey),
    OwnEntity(EntityKey, LocalEntityKey),
    DisownEntity(EntityKey, LocalEntityKey),
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

impl<P: ProtocolType> EntityAction<P> {
    pub fn as_type(&self) -> EntityActionType {
        match self {
            EntityAction::SpawnEntity(_, _, _) => EntityActionType::SpawnEntity,
            EntityAction::DespawnEntity(_, _) => EntityActionType::DespawnEntity,
            EntityAction::OwnEntity(_, _) => EntityActionType::OwnEntity,
            EntityAction::DisownEntity(_, _) => EntityActionType::DisownEntity,
            EntityAction::InsertComponent(_, _, _, _) => EntityActionType::InsertComponent,
            EntityAction::RemoveComponent(_, _) => EntityActionType::RemoveComponent,
            EntityAction::UpdateComponent(_, _, _, _) => EntityActionType::UpdateComponent,
        }
    }
}

impl<P: ProtocolType> Clone for EntityAction<P> {
    fn clone(&self) -> Self {
        match self {
            EntityAction::SpawnEntity(gk, lk, cs) => {
                EntityAction::SpawnEntity(gk.clone(), lk.clone(), cs.clone())
            }
            EntityAction::DespawnEntity(gk, lk) => {
                EntityAction::DespawnEntity(gk.clone(), lk.clone())
            }
            EntityAction::OwnEntity(gk, lk) => EntityAction::OwnEntity(gk.clone(), lk.clone()),
            EntityAction::DisownEntity(gk, lk) => {
                EntityAction::DisownEntity(gk.clone(), lk.clone())
            }
            EntityAction::InsertComponent(lek, gck, lck, r) => {
                EntityAction::InsertComponent(lek.clone(), gck.clone(), lck.clone(), r.clone())
            }
            EntityAction::RemoveComponent(gk, lk) => {
                EntityAction::RemoveComponent(gk.clone(), lk.clone())
            }
            EntityAction::UpdateComponent(gk, lk, sm, e) => {
                EntityAction::UpdateComponent(gk.clone(), lk.clone(), sm.clone(), e.clone())
            }
        }
    }
}
