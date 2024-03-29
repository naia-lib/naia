use crate::{ComponentKind, EntityAuthStatus, RemoteEntity, Replicate, Tick};

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
    EnableDelegationEntityResponse(E),
    DisableDelegationEntity(E),
    EntityRequestAuthority(E, RemoteEntity),
    EntityReleaseAuthority(E),
    EntityUpdateAuthority(E, EntityAuthStatus),
    EntityMigrateResponse(E, RemoteEntity),
}
