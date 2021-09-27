use naia_shared::{
    DiffMask, EntityActionType, LocalComponentKey, LocalEntityKey, ProtocolType, Ref, Replicate,
};

use super::{world_type::WorldType, keys::{ComponentKey, KeyType}};

#[derive(Debug)]
pub enum EntityAction<P: ProtocolType, W: WorldType<P>> {
    SpawnEntity(
        W::EntityKey,
        LocalEntityKey,
        Option<Vec<(ComponentKey<P, W>, LocalComponentKey, Ref<dyn Replicate<P>>)>>,
    ),
    DespawnEntity(W::EntityKey, LocalEntityKey),
    OwnEntity(W::EntityKey, LocalEntityKey),
    DisownEntity(W::EntityKey, LocalEntityKey),
    InsertComponent(
        LocalEntityKey,
        ComponentKey<P, W>,
        LocalComponentKey,
        Ref<dyn Replicate<P>>,
    ),
    UpdateComponent(
        ComponentKey<P, W>,
        LocalComponentKey,
        Ref<DiffMask>,
        Ref<dyn Replicate<P>>,
    ),
    RemoveComponent(ComponentKey<P, W>, LocalComponentKey),
}

impl<P: ProtocolType, W: WorldType<P>> EntityAction<P, W> {
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

impl<P: ProtocolType, W: WorldType<P>> Clone for EntityAction<P, W> {
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
