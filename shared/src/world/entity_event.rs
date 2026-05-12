use crate::{
    ComponentKind, EntityAuthStatus, EntityMessageType, GlobalEntity, RemoteEntity, Replicate, Tick,
};

/// ECS-level event produced by the replication system when the remote world state changes.
pub enum EntityEvent {
    /// A new entity was spawned by the remote.
    Spawn(GlobalEntity),
    /// An existing entity was despawned by the remote.
    Despawn(GlobalEntity),
    /// A component was added to an entity.
    InsertComponent(GlobalEntity, ComponentKind),
    /// A component was removed from an entity; carries the last known component value.
    RemoveComponent(GlobalEntity, Box<dyn Replicate>),
    /// A component on an entity was updated at the given tick.
    UpdateComponent(Tick, GlobalEntity, ComponentKind),

    /// Entity was published (made visible to other users).
    Publish(GlobalEntity),
    /// Entity publication was retracted.
    Unpublish(GlobalEntity),
    /// Authority delegation was enabled for an entity.
    EnableDelegation(GlobalEntity),
    /// Authority delegation was disabled for an entity.
    DisableDelegation(GlobalEntity),
    /// Authority status for a delegated entity was updated.
    SetAuthority(GlobalEntity, EntityAuthStatus),

    /// Client is requesting authority over an entity.
    RequestAuthority(GlobalEntity),
    /// Client is releasing authority over an entity.
    ReleaseAuthority(GlobalEntity),
    /// Client acknowledged that delegation is now enabled.
    EnableDelegationResponse(GlobalEntity),
    /// An entity migrated; carries the new remote entity ID.
    MigrateResponse(GlobalEntity, RemoteEntity),
}

impl EntityEvent {
    /// Returns the [`EntityMessageType`] discriminant for this event, or `None` for `UpdateComponent` (which has no wire type).
    pub fn to_type(&self) -> Option<EntityMessageType> {
        match self {
            Self::Spawn(_) => Some(EntityMessageType::Spawn),
            Self::Despawn(_) => Some(EntityMessageType::Despawn),
            Self::InsertComponent(_, _) => Some(EntityMessageType::InsertComponent),
            Self::RemoveComponent(_, _) => Some(EntityMessageType::RemoveComponent),
            Self::Publish(_) => Some(EntityMessageType::Publish),
            Self::Unpublish(_) => Some(EntityMessageType::Unpublish),
            Self::EnableDelegation(_) => Some(EntityMessageType::EnableDelegation),
            Self::EnableDelegationResponse(_) => Some(EntityMessageType::EnableDelegationResponse),
            Self::DisableDelegation(_) => Some(EntityMessageType::DisableDelegation),
            Self::RequestAuthority(_) => Some(EntityMessageType::RequestAuthority),
            Self::ReleaseAuthority(_) => Some(EntityMessageType::ReleaseAuthority),
            Self::SetAuthority(_, _) => Some(EntityMessageType::SetAuthority),
            Self::MigrateResponse(_, _) => Some(EntityMessageType::MigrateResponse),
            Self::UpdateComponent(_, _, _) => None, // UpdateComponent is not a message type
        }
    }

    /// Returns the [`GlobalEntity`] this event refers to.
    pub fn entity(&self) -> GlobalEntity {
        match self {
            Self::Spawn(entity) => *entity,
            Self::Despawn(entity) => *entity,
            Self::InsertComponent(entity, _) => *entity,
            Self::RemoveComponent(entity, _) => *entity,
            Self::UpdateComponent(_, entity, _) => *entity,
            Self::Publish(entity) => *entity,
            Self::Unpublish(entity) => *entity,
            Self::EnableDelegation(entity) => *entity,
            Self::EnableDelegationResponse(entity) => *entity,
            Self::DisableDelegation(entity) => *entity,
            Self::RequestAuthority(entity) => *entity,
            Self::ReleaseAuthority(entity) => *entity,
            Self::SetAuthority(entity, _) => *entity,
            Self::MigrateResponse(entity, _) => *entity,
        }
    }

    /// Returns a human-readable string describing this event, suitable for debug logging.
    pub fn log(&self) -> String {
        let entity = self.entity();
        if let Some(ev_type) = self.to_type() {
            format!("{:?} {:?}", ev_type, entity)
        } else {
            format!("UpdateComponent {:?}", entity)
        }
    }
}
