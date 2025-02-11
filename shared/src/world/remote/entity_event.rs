use crate::{ComponentKind, EntityAuthStatus, GlobalEntity, RemoteEntity, Replicate, Tick};

pub enum EntityEvent {
    SpawnEntity(GlobalEntity),
    DespawnEntity(GlobalEntity),
    InsertComponent(GlobalEntity, ComponentKind),
    RemoveComponent(GlobalEntity, Box<dyn Replicate>),
    UpdateComponent(Tick, GlobalEntity, ComponentKind),
}

pub enum EntityResponseEvent {
    SpawnEntity(GlobalEntity),
    DespawnEntity(GlobalEntity),
    InsertComponent(GlobalEntity, ComponentKind),
    RemoveComponent(GlobalEntity, ComponentKind),
    PublishEntity(GlobalEntity),
    UnpublishEntity(GlobalEntity),
    EnableDelegationEntity(GlobalEntity),
    EnableDelegationEntityResponse(GlobalEntity),
    DisableDelegationEntity(GlobalEntity),
    EntityRequestAuthority(GlobalEntity, RemoteEntity),
    EntityReleaseAuthority(GlobalEntity),
    EntityUpdateAuthority(GlobalEntity, EntityAuthStatus),
    EntityMigrateResponse(GlobalEntity, RemoteEntity),
}
