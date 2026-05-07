use log::error;

use crate::{
    world::component::component_kinds::ComponentKind,
    world::host::host_world_manager::SubCommandId, EntityAuthStatus, EntityEvent,
    EntityMessageType, HostEntity, LocalEntityMap, RemoteEntity,
};

// Raw entity sync messages sent over the wire
#[derive(PartialEq, Eq, Debug, Clone)]
pub enum EntityMessage<E: Copy + Eq + PartialEq> {
    Spawn(E),
    SpawnWithComponents(E, Vec<ComponentKind>),
    Despawn(E),
    InsertComponent(E, ComponentKind),
    RemoveComponent(E, ComponentKind),
    Publish(SubCommandId, E),
    Unpublish(SubCommandId, E),
    EnableDelegation(SubCommandId, E),
    DisableDelegation(SubCommandId, E),
    SetAuthority(SubCommandId, E, EntityAuthStatus),

    // These are not commands, they are something else
    RequestAuthority(SubCommandId, E),
    ReleaseAuthority(SubCommandId, E),
    EnableDelegationResponse(SubCommandId, E),
    MigrateResponse(SubCommandId, E, RemoteEntity),

    Noop,
}

impl<E: Copy + Eq + PartialEq> EntityMessage<E> {
    pub fn entity(&self) -> Option<E> {
        match self {
            Self::Spawn(entity) => Some(*entity),
            Self::SpawnWithComponents(entity, _) => Some(*entity),
            Self::Despawn(entity) => Some(*entity),
            Self::InsertComponent(entity, _) => Some(*entity),
            Self::RemoveComponent(entity, _) => Some(*entity),
            Self::Publish(_, entity) => Some(*entity),
            Self::Unpublish(_, entity) => Some(*entity),
            Self::EnableDelegation(_, entity) => Some(*entity),
            Self::EnableDelegationResponse(_, entity) => Some(*entity),
            Self::DisableDelegation(_, entity) => Some(*entity),
            Self::RequestAuthority(_, entity) => Some(*entity),
            Self::ReleaseAuthority(_, entity) => Some(*entity),
            Self::SetAuthority(_, entity, _) => Some(*entity),
            Self::MigrateResponse(_, entity, _) => Some(*entity),
            Self::Noop => None,
        }
    }

    pub fn is_noop(&self) -> bool {
        matches!(self, Self::Noop)
    }

    pub fn component_kind(&self) -> Option<ComponentKind> {
        match self {
            Self::InsertComponent(_, component_kind) => Some(*component_kind),
            Self::RemoveComponent(_, component_kind) => Some(*component_kind),
            _ => None,
        }
    }

    pub fn strip_entity(self) -> EntityMessage<()> {
        match self {
            Self::Spawn(_) => EntityMessage::Spawn(()),
            Self::SpawnWithComponents(_, kinds) => EntityMessage::SpawnWithComponents((), kinds),
            Self::Despawn(_) => EntityMessage::Despawn(()),
            Self::InsertComponent(_, component_kind) => {
                EntityMessage::InsertComponent((), component_kind)
            }
            Self::RemoveComponent(_, component_kind) => {
                EntityMessage::RemoveComponent((), component_kind)
            }
            Self::Publish(sub_id, _) => EntityMessage::Publish(sub_id, ()),
            Self::Unpublish(sub_id, _) => EntityMessage::Unpublish(sub_id, ()),
            Self::EnableDelegation(sub_id, _) => EntityMessage::EnableDelegation(sub_id, ()),
            Self::EnableDelegationResponse(sub_id, _) => {
                EntityMessage::EnableDelegationResponse(sub_id, ())
            }
            Self::DisableDelegation(sub_id, _) => EntityMessage::DisableDelegation(sub_id, ()),
            Self::RequestAuthority(sub_id, _) => EntityMessage::RequestAuthority(sub_id, ()),
            Self::ReleaseAuthority(sub_id, _) => EntityMessage::ReleaseAuthority(sub_id, ()),
            Self::SetAuthority(sub_id, _, status) => {
                EntityMessage::SetAuthority(sub_id, (), status)
            }
            Self::MigrateResponse(sub_id, _, other_entity) => {
                EntityMessage::MigrateResponse(sub_id, (), other_entity)
            }
            Self::Noop => panic!("Cannot strip entity from a Noop message"),
        }
    }

    pub fn with_entity<O: Copy + Eq + PartialEq>(self, entity: O) -> EntityMessage<O> {
        match self {
            EntityMessage::Spawn(_) => EntityMessage::Spawn(entity),
            EntityMessage::SpawnWithComponents(_, kinds) => EntityMessage::SpawnWithComponents(entity, kinds),
            EntityMessage::Despawn(_) => EntityMessage::Despawn(entity),
            EntityMessage::InsertComponent(_, component_kind) => {
                EntityMessage::InsertComponent(entity, component_kind)
            }
            EntityMessage::RemoveComponent(_, component_kind) => {
                EntityMessage::RemoveComponent(entity, component_kind)
            }
            EntityMessage::Publish(sub_id, _) => EntityMessage::Publish(sub_id, entity),
            EntityMessage::Unpublish(sub_id, _) => EntityMessage::Unpublish(sub_id, entity),
            EntityMessage::EnableDelegation(sub_id, _) => {
                EntityMessage::EnableDelegation(sub_id, entity)
            }
            EntityMessage::EnableDelegationResponse(sub_id, _) => {
                EntityMessage::EnableDelegationResponse(sub_id, entity)
            }
            EntityMessage::DisableDelegation(sub_id, _) => {
                EntityMessage::DisableDelegation(sub_id, entity)
            }
            EntityMessage::RequestAuthority(sub_id, _) => {
                EntityMessage::RequestAuthority(sub_id, entity)
            }
            EntityMessage::ReleaseAuthority(sub_id, _) => {
                EntityMessage::ReleaseAuthority(sub_id, entity)
            }
            EntityMessage::SetAuthority(sub_id, _, status) => {
                EntityMessage::SetAuthority(sub_id, entity, status)
            }
            EntityMessage::MigrateResponse(sub_id, _, other_entity) => {
                EntityMessage::MigrateResponse(sub_id, entity, other_entity)
            }
            EntityMessage::Noop => panic!("Cannot add entity to a Noop message"),
        }
    }

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
            Self::EnableDelegationResponse(_, _) => EntityMessageType::EnableDelegationResponse,
            Self::DisableDelegation(_, _) => EntityMessageType::DisableDelegation,
            Self::RequestAuthority(_, _) => EntityMessageType::RequestAuthority,
            Self::ReleaseAuthority(_, _) => EntityMessageType::ReleaseAuthority,
            Self::SetAuthority(_, _, _) => EntityMessageType::SetAuthority,
            Self::MigrateResponse(_, _, _) => EntityMessageType::MigrateResponse,
            Self::Noop => EntityMessageType::Noop,
        }
    }

    pub fn subcommand_id(&self) -> Option<SubCommandId> {
        match self {
            Self::Publish(sub_id, _) => Some(*sub_id),
            Self::Unpublish(sub_id, _) => Some(*sub_id),
            Self::EnableDelegation(sub_id, _) => Some(*sub_id),
            Self::EnableDelegationResponse(sub_id, _) => Some(*sub_id),
            Self::DisableDelegation(sub_id, _) => Some(*sub_id),
            Self::RequestAuthority(sub_id, _) => Some(*sub_id),
            Self::ReleaseAuthority(sub_id, _) => Some(*sub_id),
            Self::SetAuthority(sub_id, _, _) => Some(*sub_id),
            Self::MigrateResponse(sub_id, _, _) => Some(*sub_id),
            _ => None,
        }
    }

    pub fn apply_entity_redirect<O: Copy + Eq + PartialEq>(
        self,
        old_entity: &E,
        new_entity: &O,
    ) -> EntityMessage<O> {
        if let Some(entity) = self.entity() {
            if entity == *old_entity {
                return self.with_entity(*new_entity);
            }
        }
        // If no entity or entity doesn't match, return a message with the new entity
        self.with_entity(*new_entity)
    }
}
//
impl EntityMessage<RemoteEntity> {
    //
    //     pub fn to_host_message(self) -> EntityMessage<HostEntity> {
    //         match self {
    //             EntityMessage::EnableDelegationResponse(sub_id, entity) => {
    //                 EntityMessage::EnableDelegationResponse(sub_id, entity.to_host())
    //             }
    //             EntityMessage::MigrateResponse(sub_id, entity, other_entity) => {
    //                 EntityMessage::MigrateResponse(sub_id, entity.to_host(), other_entity)
    //             }
    //             EntityMessage::RequestAuthority(sub_id, entity) => {
    //                 EntityMessage::RequestAuthority(sub_id, entity.to_host())
    //             }
    //             EntityMessage::ReleaseAuthority(_, _) => panic!("EntityReleaseAuthority should not call `to_host_message()`"),
    //             msg => {
    //                 panic!("No reason to convert message {:?} to HostEntity", msg);
    //             }
    //         }
    //     }
    //
    pub fn to_event(self, local_entity_map: &LocalEntityMap) -> EntityEvent {
        let remote_entity = self.entity().unwrap();
        let global_entity = match local_entity_map.global_entity_from_remote(&remote_entity) {
            Some(ge) => *ge,
            None => {
                error!("to_event() failed to find RemoteEntity({:?}) in entity_map! Message type: {:?}", 
                    remote_entity, self.get_type());
                panic!("RemoteEntity not found in entity_map during to_event conversion");
            }
        };
        match self {
            EntityMessage::Publish(_, _) => EntityEvent::Publish(global_entity),
            EntityMessage::Unpublish(_, _) => EntityEvent::Unpublish(global_entity),
            EntityMessage::EnableDelegation(_, _) => EntityEvent::EnableDelegation(global_entity),
            EntityMessage::EnableDelegationResponse(_, _) => {
                panic!("EnableDelegationResponse should not be sent by remote")
            }
            EntityMessage::DisableDelegation(_, _) => EntityEvent::DisableDelegation(global_entity),
            EntityMessage::RequestAuthority(_, _) => EntityEvent::RequestAuthority(global_entity),
            EntityMessage::ReleaseAuthority(_, _) => EntityEvent::ReleaseAuthority(global_entity),
            EntityMessage::SetAuthority(_, _, status) => {
                EntityEvent::SetAuthority(global_entity, status)
            }
            EntityMessage::MigrateResponse(_, _, _new_remote_entity) => {
                // MigrateResponse should never be EntityMessage<RemoteEntity>!
                // It should be EntityMessage<HostEntity> so the client can look it up
                panic!("MigrateResponse should be EntityMessage<HostEntity>, not EntityMessage<RemoteEntity>!");
            }
            EntityMessage::Spawn(_)
            | EntityMessage::SpawnWithComponents(_, _)
            | EntityMessage::Despawn(_)
            | EntityMessage::InsertComponent(_, _)
            | EntityMessage::RemoveComponent(_, _) => panic!("Handled elsewhere"),
            EntityMessage::Noop => panic!("Cannot convert Noop message to an event"),
        }
    }
}
//
impl EntityMessage<HostEntity> {
    pub fn to_event(self, local_entity_map: &LocalEntityMap) -> Option<EntityEvent> {
        let host_entity = self.entity().unwrap();
        let global_entity = match local_entity_map.global_entity_from_host(&host_entity) {
            Some(ge) => *ge,
            None => {
                error!(
                    "to_event() failed to find HostEntity({:?}) in entity_map — message type: {:?}; skipping",
                    host_entity,
                    self.get_type()
                );
                return None;
            }
        };
        Some(match self {
            EntityMessage::Publish(_, _) => EntityEvent::Publish(global_entity),
            EntityMessage::Unpublish(_, _) => EntityEvent::Unpublish(global_entity),
            EntityMessage::EnableDelegation(_, _) => EntityEvent::EnableDelegation(global_entity),
            EntityMessage::EnableDelegationResponse(_, _) => {
                EntityEvent::EnableDelegationResponse(global_entity)
            }
            EntityMessage::DisableDelegation(_, _) => EntityEvent::DisableDelegation(global_entity),
            EntityMessage::RequestAuthority(_, _) => EntityEvent::RequestAuthority(global_entity),
            EntityMessage::ReleaseAuthority(_, _) => EntityEvent::ReleaseAuthority(global_entity),
            EntityMessage::SetAuthority(_, _, status) => {
                EntityEvent::SetAuthority(global_entity, status)
            }
            EntityMessage::MigrateResponse(_, _, new_remote_entity) => {
                EntityEvent::MigrateResponse(global_entity, new_remote_entity)
            }
            EntityMessage::Spawn(_)
            | EntityMessage::SpawnWithComponents(_, _)
            | EntityMessage::Despawn(_)
            | EntityMessage::InsertComponent(_, _)
            | EntityMessage::RemoveComponent(_, _) => panic!("Handled elsewhere"),
            EntityMessage::Noop => panic!("Cannot convert Noop message to an event"),
        })
    }
}
