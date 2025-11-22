use crate::{
    ComponentKind, EntityAuthStatus, EntityMessageType, GlobalEntity, RemoteEntity, Replicate, Tick,
};

// ECS Lifecycle Events
pub enum EntityEvent {
    Spawn(GlobalEntity),
    Despawn(GlobalEntity),
    InsertComponent(GlobalEntity, ComponentKind),
    RemoveComponent(GlobalEntity, Box<dyn Replicate>),
    UpdateComponent(Tick, GlobalEntity, ComponentKind),

    Publish(GlobalEntity),
    Unpublish(GlobalEntity),
    EnableDelegation(GlobalEntity),
    DisableDelegation(GlobalEntity),
    SetAuthority(GlobalEntity, EntityAuthStatus),

    // These are not commands, they are something else
    RequestAuthority(GlobalEntity),
    ReleaseAuthority(GlobalEntity),
    EnableDelegationResponse(GlobalEntity),
    MigrateResponse(GlobalEntity, RemoteEntity),
}

impl EntityEvent {
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

    pub fn log(&self) -> String {
        let entity = self.entity();
        if let Some(ev_type) = self.to_type() {
            format!("{:?} {:?}", ev_type, entity)
        } else {
            format!("UpdateComponent {:?}", entity)
        }
    }
}
