use crate::{ComponentKind, Replicate, Tick};

pub enum EntityEvent<E: Copy> {
    SpawnEntity(E),
    DespawnEntity(E),
    PublishEntity(E),
    InsertComponent(E, ComponentKind),
    RemoveComponent(E, Box<dyn Replicate>),
    UpdateComponent(Tick, E, ComponentKind),
}
