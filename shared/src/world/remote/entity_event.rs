use crate::{ComponentKind, Replicate, Tick};

pub enum EntityEvent<E: Copy> {
    SpawnEntity(E),
    DespawnEntity(E),
    InsertComponent(E, ComponentKind),
    RemoveComponent(E, Box<dyn Replicate>),
    UpdateComponent(Tick, E, ComponentKind),
}

pub enum EntityResponseEvent<E: Copy> {
    SpawnEntity(E),
    DespawnEntity(E),
    InsertComponent(E, ComponentKind),
    RemoveComponent(E, ComponentKind),
    PublishEntity(E),
    UnpublishEntity(E),
    EnableDelegationEntity(E),
    DisableDelegationEntity(E),
}
