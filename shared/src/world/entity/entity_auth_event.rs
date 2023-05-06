use std::hash::Hash;

use naia_derive::MessageInternal;
use naia_serde::SerdeInternal;

use crate::{
    EntityAndGlobalEntityConverter, EntityAuthStatus, EntityProperty, EntityResponseEvent,
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
    RequestAuthority,
    ReleaseAuthority,
    UpdateAuthority(EntityAuthStatus),
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
                EntityResponseEvent::EnableDelegationEntity(*entity)
            }
            EntityEventMessageAction::RequestAuthority => {
                EntityResponseEvent::EntityRequestAuthority(*entity)
            }
            EntityEventMessageAction::ReleaseAuthority => {
                EntityResponseEvent::EntityReleaseAuthority(*entity)
            }
            EntityEventMessageAction::UpdateAuthority(new_auth_status) => {
                EntityResponseEvent::EntityUpdateAuthority(*entity, *new_auth_status)
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
    ) -> Self {
        Self::new(
            converter,
            entity,
            EntityEventMessageAction::RequestAuthority,
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
