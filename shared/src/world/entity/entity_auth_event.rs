
use std::hash::Hash;

use naia_derive::MessageInternal;
use naia_serde::SerdeInternal;

use crate::{EntityAndGlobalEntityConverter, EntityProperty, EntityResponseEvent};

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
    DisableDelegation,
    RequestAuthority,
    ReleaseAuthority,
}

impl EntityEventMessageAction {
    pub fn to_response_event<E: Copy>(&self, entity: &E) -> Option<EntityResponseEvent<E>> {
        match self {
            EntityEventMessageAction::Publish => {
                Some(EntityResponseEvent::PublishEntity(*entity))
            },
            EntityEventMessageAction::Unpublish => {
                Some(EntityResponseEvent::UnpublishEntity(*entity))
            },
            EntityEventMessageAction::EnableDelegation => {
                Some(EntityResponseEvent::EnableDelegationEntity(*entity))
            }
            EntityEventMessageAction::DisableDelegation => {
                Some(EntityResponseEvent::EnableDelegationEntity(*entity))
            }
            EntityEventMessageAction::RequestAuthority => {
                // don't need to process this, as the origin of the action is always the host
                todo!()
            }
            EntityEventMessageAction::ReleaseAuthority => {
                // don't need to process this, as the origin of the action is always the host
                todo!()
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
        Self::new(converter, entity, EntityEventMessageAction::EnableDelegation)
    }

    pub fn new_disable_delegation<E: Copy + Eq + Hash + Send + Sync>(
        converter: &dyn EntityAndGlobalEntityConverter<E>,
        entity: &E,
    ) -> Self {
        Self::new(converter, entity, EntityEventMessageAction::DisableDelegation)
    }

    pub fn new_request_authority<E: Copy + Eq + Hash + Send + Sync>(
        converter: &dyn EntityAndGlobalEntityConverter<E>,
        entity: &E,
    ) -> Self {
        Self::new(converter, entity, EntityEventMessageAction::RequestAuthority)
    }

    pub fn new_release_authority<E: Copy + Eq + Hash + Send + Sync>(
        converter: &dyn EntityAndGlobalEntityConverter<E>,
        entity: &E,
    ) -> Self {
        Self::new(converter, entity, EntityEventMessageAction::ReleaseAuthority)
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
