use crate::{Tick, ComponentKind, Replicate};

pub enum EntityEvent<E: Copy> {
    SpawnEntity(E),
    DespawnEntity(E),
    InsertComponent(E, ComponentKind),
    RemoveComponent(E, Box<dyn Replicate>),
    UpdateComponent(Tick, E, ComponentKind)
}