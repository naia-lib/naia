use crate::{
    world::host::host_world_manager::SubCommandId, ComponentKind, EntityAuthStatus,
    EntityMessageType, GlobalEntity, HostEntity, RemoteEntity,
};

// TODO! make this agnostic to type of entity

// command to sync entities from host -> remote
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum EntityCommand {
    Spawn(GlobalEntity),
    Despawn(GlobalEntity),
    InsertComponent(GlobalEntity, ComponentKind),
    RemoveComponent(GlobalEntity, ComponentKind),

    // Former SystemChannel messages
    Publish(Option<SubCommandId>, GlobalEntity),
    Unpublish(Option<SubCommandId>, GlobalEntity),
    EnableDelegation(Option<SubCommandId>, GlobalEntity),
    DisableDelegation(Option<SubCommandId>, GlobalEntity), // only sent by server
    SetAuthority(Option<SubCommandId>, GlobalEntity, EntityAuthStatus), // only sent by server

    // These aren't commands, they are something else
    RequestAuthority(Option<SubCommandId>, GlobalEntity), // only sent by client
    ReleaseAuthority(Option<SubCommandId>, GlobalEntity), // only sent by client
    EnableDelegationResponse(Option<SubCommandId>, GlobalEntity), // only sent by client
    MigrateResponse(Option<SubCommandId>, GlobalEntity, RemoteEntity, HostEntity), // only sent by server: (subid, global, old_remote, new_host)
}

impl EntityCommand {
    pub fn entity(&self) -> GlobalEntity {
        match self {
            Self::Spawn(entity) => *entity,
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

    pub fn component_kind(&self) -> Option<ComponentKind> {
        match self {
            Self::InsertComponent(_, component_kind) => Some(*component_kind),
            Self::RemoveComponent(_, component_kind) => Some(*component_kind),
            _ => None,
        }
    }

    pub fn get_type(&self) -> EntityMessageType {
        match self {
            Self::Spawn(_) => EntityMessageType::Spawn,
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

    pub fn is_valid_for_remote_entity(&self) -> bool {
        // During client-side migration, some commands become invalid
        // Publish/Unpublish don't make sense for delegated entities
        // Delegation commands don't make sense post-delegation
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
