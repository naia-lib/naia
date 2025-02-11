use crate::{ComponentKind, GlobalEntity};

#[derive(Clone, PartialEq, Eq)]
pub enum EntityActionEvent {
    SpawnEntity(GlobalEntity, Vec<ComponentKind>),
    DespawnEntity(GlobalEntity),
    InsertComponent(GlobalEntity, ComponentKind),
    RemoveComponent(GlobalEntity, ComponentKind),
}
