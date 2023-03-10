use crate::{ComponentKind, Replicate};

pub enum EntityActionEvent<E: Copy> {
    SpawnEntity(E),
    DespawnEntity(E),
    InsertComponent(E, ComponentKind),
    RemoveComponent(E, Box<dyn Replicate>),
}
