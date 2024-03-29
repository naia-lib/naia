use std::hash::Hash;

use naia_derive::MessageInternal;
use naia_serde::SerdeInternal;

use crate::{
    EntityAndGlobalEntityConverter, EntityAuthStatus, EntityProperty, EntityResponseEvent,
    HostEntity, RemoteEntity,
};

#[derive(MessageInternal)]
pub struct EntityEventMessage {
    pub entity: EntityProperty,
    pub action: EntityEventMessageAction,
}

#[derive(SerdeInternal, Clone, Debug, PartialEq)]
pub enum EntityEventMessageAction {
    Publish,
    Unpublish,
    EnableDelegation,
    EnableDelegationResponse,
    DisableDelegation,
    RequestAuthority(u16),
    ReleaseAuthority,
    UpdateAuthority(EntityAuthStatus),
    EntityMigrateResponse(u16), //u16 here is new Host Entity
}

impl EntityEventMessageAction {
    pub fn to_response_event<E: Copy>(&self, entity: &E) -> EntityResponseEvent<E> {
        match self {
            EntityEventMessageAction::Publish => EntityResponseEvent::PublishEntity(*entity),
            EntityEventMessageAction::Unpublish => EntityResponseEvent::UnpublishEntity(*entity),
            EntityEventMessageAction::EnableDelegation => {
                EntityResponseEvent::EnableDelegationEntity(*entity)
            }
            EntityEventMessageAction::EnableDelegationResponse => {
                EntityResponseEvent::EnableDelegationEntityResponse(*entity)
            }
            EntityEventMessageAction::DisableDelegation => {
                EntityResponseEvent::DisableDelegationEntity(*entity)
            }
            EntityEventMessageAction::RequestAuthority(remote_entity) => {
                EntityResponseEvent::EntityRequestAuthority(
                    *entity,
                    RemoteEntity::new(*remote_entity),
                )
            }
            EntityEventMessageAction::ReleaseAuthority => {
                EntityResponseEvent::EntityReleaseAuthority(*entity)
            }
            EntityEventMessageAction::UpdateAuthority(new_auth_status) => {
                EntityResponseEvent::EntityUpdateAuthority(*entity, *new_auth_status)
            }
            EntityEventMessageAction::EntityMigrateResponse(remote_entity) => {
                EntityResponseEvent::EntityMigrateResponse(
                    *entity,
                    RemoteEntity::new(*remote_entity),
                )
            }
        }
    }
}

impl EntityEventMessage {
    pub fn new_publish<E: Copy + Eq + Hash + Send + Sync>(
        converter: &dyn EntityAndGlobalEntityConverter<E>,
        entity: &E,
    ) -> Self {
        Self::new(converter, entity, EntityEventMessageAction::Publish)
    }

    pub fn new_unpublish<E: Copy + Eq + Hash + Send + Sync>(
        converter: &dyn EntityAndGlobalEntityConverter<E>,
        entity: &E,
    ) -> Self {
        Self::new(converter, entity, EntityEventMessageAction::Unpublish)
    }

    pub fn new_enable_delegation<E: Copy + Eq + Hash + Send + Sync>(
        converter: &dyn EntityAndGlobalEntityConverter<E>,
        entity: &E,
    ) -> Self {
        Self::new(
            converter,
            entity,
            EntityEventMessageAction::EnableDelegation,
        )
    }

    pub fn new_enable_delegation_response<E: Copy + Eq + Hash + Send + Sync>(
        converter: &dyn EntityAndGlobalEntityConverter<E>,
        entity: &E,
    ) -> Self {
        Self::new(
            converter,
            entity,
            EntityEventMessageAction::EnableDelegationResponse,
        )
    }

    pub fn new_disable_delegation<E: Copy + Eq + Hash + Send + Sync>(
        converter: &dyn EntityAndGlobalEntityConverter<E>,
        entity: &E,
    ) -> Self {
        Self::new(
            converter,
            entity,
            EntityEventMessageAction::DisableDelegation,
        )
    }

    pub fn new_request_authority<E: Copy + Eq + Hash + Send + Sync>(
        converter: &dyn EntityAndGlobalEntityConverter<E>,
        entity: &E,
        host_entity: HostEntity,
    ) -> Self {
        Self::new(
            converter,
            entity,
            EntityEventMessageAction::RequestAuthority(host_entity.value()),
        )
    }

    pub fn new_release_authority<E: Copy + Eq + Hash + Send + Sync>(
        converter: &dyn EntityAndGlobalEntityConverter<E>,
        entity: &E,
    ) -> Self {
        Self::new(
            converter,
            entity,
            EntityEventMessageAction::ReleaseAuthority,
        )
    }

    pub fn new_update_auth_status<E: Copy + Eq + Hash + Send + Sync>(
        converter: &dyn EntityAndGlobalEntityConverter<E>,
        entity: &E,
        auth_status: EntityAuthStatus,
    ) -> Self {
        Self::new(
            converter,
            entity,
            EntityEventMessageAction::UpdateAuthority(auth_status),
        )
    }

    pub fn new_entity_migrate_response<E: Copy + Eq + Hash + Send + Sync>(
        converter: &dyn EntityAndGlobalEntityConverter<E>,
        entity: &E,
        host_entity: HostEntity,
    ) -> Self {
        Self::new(
            converter,
            entity,
            EntityEventMessageAction::EntityMigrateResponse(host_entity.value()),
        )
    }

    fn new<E: Copy + Eq + Hash + Send + Sync>(
        converter: &dyn EntityAndGlobalEntityConverter<E>,
        entity: &E,
        action: EntityEventMessageAction,
    ) -> Self {
        let mut output = Self {
            entity: EntityProperty::new(),
            action,
        };

        output.entity.set(converter, entity);

        output
    }
}
