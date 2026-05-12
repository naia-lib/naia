use crate::{
    world::host::host_world_manager::SubCommandId, ComponentKind, EntityAuthStatus,
    EntityMessageType, GlobalEntity, HostEntity, RemoteEntity,
};

// TODO! make this agnostic to type of entity

/// Wire command syncing entity lifecycle and authority transitions from host to remote.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum EntityCommand {
    /// Spawn an entity with no initial components.
    Spawn(GlobalEntity),
    /// Spawn an entity pre-loaded with the listed component kinds.
    SpawnWithComponents(GlobalEntity, Vec<ComponentKind>),
    /// Despawn an existing entity.
    Despawn(GlobalEntity),
    /// Insert a component onto an existing entity.
    InsertComponent(GlobalEntity, ComponentKind),
    /// Remove a component from an existing entity.
    RemoveComponent(GlobalEntity, ComponentKind),

    /// Publish a delegated entity so it becomes visible to other users.
    Publish(Option<SubCommandId>, GlobalEntity),
    /// Retract a previously published entity.
    Unpublish(Option<SubCommandId>, GlobalEntity),
    /// Enable client-authority delegation for an entity.
    EnableDelegation(Option<SubCommandId>, GlobalEntity),
    /// Revoke client-authority delegation (server only).
    DisableDelegation(Option<SubCommandId>, GlobalEntity),
    /// Update the authority status for a delegated entity (server only).
    SetAuthority(Option<SubCommandId>, GlobalEntity, EntityAuthStatus),

    /// Client requests authority over a delegated entity.
    RequestAuthority(Option<SubCommandId>, GlobalEntity),
    /// Client releases previously held authority.
    ReleaseAuthority(Option<SubCommandId>, GlobalEntity),
    /// Client acknowledges that delegation has been enabled.
    EnableDelegationResponse(Option<SubCommandId>, GlobalEntity),
    /// Server notifies that an entity has migrated from remote to host (subid, global, old_remote, new_host).
    MigrateResponse(Option<SubCommandId>, GlobalEntity, RemoteEntity, HostEntity),
}

impl EntityCommand {
    /// Returns the primary `GlobalEntity` this command targets.
    pub fn entity(&self) -> GlobalEntity {
        match self {
            Self::Spawn(entity) => *entity,
            Self::SpawnWithComponents(entity, _) => *entity,
            Self::Despawn(entity) => *entity,
            Self::InsertComponent(entity, _) => *entity,
            Self::RemoveComponent(entity, _) => *entity,
            Self::Publish(_, entity) => *entity,
            Self::Unpublish(_, entity) => *entity,
            Self::EnableDelegation(_, entity) => *entity,
            Self::DisableDelegation(_, entity) => *entity,
            Self::SetAuthority(_, entity, _) => *entity,
            Self::RequestAuthority(_, entity) => *entity,
            Self::ReleaseAuthority(_, entity) => *entity,
            Self::EnableDelegationResponse(_, entity) => *entity,
            Self::MigrateResponse(_, entity, _, _) => *entity,
        }
    }

    /// Returns the `ComponentKind` for insert/remove commands, or `None` for all other variants.
    pub fn component_kind(&self) -> Option<ComponentKind> {
        match self {
            Self::InsertComponent(_, component_kind) => Some(*component_kind),
            Self::RemoveComponent(_, component_kind) => Some(*component_kind),
            _ => None,
        }
    }

    /// Returns the `EntityMessageType` discriminant for this command.
    pub fn get_type(&self) -> EntityMessageType {
        match self {
            Self::Spawn(_) => EntityMessageType::Spawn,
            Self::SpawnWithComponents(_, _) => EntityMessageType::SpawnWithComponents,
            Self::Despawn(_) => EntityMessageType::Despawn,
            Self::InsertComponent(_, _) => EntityMessageType::InsertComponent,
            Self::RemoveComponent(_, _) => EntityMessageType::RemoveComponent,
            Self::Publish(_, _) => EntityMessageType::Publish,
            Self::Unpublish(_, _) => EntityMessageType::Unpublish,
            Self::EnableDelegation(_, _) => EntityMessageType::EnableDelegation,
            Self::DisableDelegation(_, _) => EntityMessageType::DisableDelegation,
            Self::SetAuthority(_, _, _) => EntityMessageType::SetAuthority,
            Self::RequestAuthority(_, _) => EntityMessageType::RequestAuthority,
            Self::ReleaseAuthority(_, _) => EntityMessageType::ReleaseAuthority,
            Self::EnableDelegationResponse(_, _) => EntityMessageType::EnableDelegationResponse,
            Self::MigrateResponse(_, _, _, _) => EntityMessageType::MigrateResponse,
        }
    }

    pub(crate) fn set_subcommand_id(&mut self, id: SubCommandId) {
        match self {
            Self::Spawn(_)
            | Self::SpawnWithComponents(_, _)
            | Self::Despawn(_)
            | Self::InsertComponent(_, _)
            | Self::RemoveComponent(_, _) => {
                panic!("Cannot set subcommand ID for a command that does not have one");
            }
            Self::Publish(sub_id, _)
            | Self::Unpublish(sub_id, _)
            | Self::EnableDelegation(sub_id, _)
            | Self::DisableDelegation(sub_id, _)
            | Self::SetAuthority(sub_id, _, _)
            | Self::RequestAuthority(sub_id, _)
            | Self::ReleaseAuthority(sub_id, _)
            | Self::EnableDelegationResponse(sub_id, _)
            | Self::MigrateResponse(sub_id, _, _, _) => {
                *sub_id = Some(id);
            }
        }
    }

    /// Returns `true` if this command can be applied to a remote (client-owned) entity.
    pub fn is_valid_for_remote_entity(&self) -> bool {
        match self {
            Self::Publish(_, _)
            | Self::Unpublish(_, _)
            | Self::EnableDelegation(_, _)
            | Self::DisableDelegation(_, _) => false,

            Self::InsertComponent(_, _) | Self::RemoveComponent(_, _) | Self::Despawn(_) => true,

            _ => false,
        }
    }
}
