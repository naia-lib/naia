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
}

impl EntityEventMessageAction {
    pub fn to_response_event<E: Copy>(&self, entity: &E) -> EntityResponseEvent<E> {
        match self {
            EntityEventMessageAction::Publish => EntityResponseEvent::PublishEntity(*entity),
            EntityEventMessageAction::Unpublish => EntityResponseEvent::UnpublishEntity(*entity),
            EntityEventMessageAction::EnableDelegation => EntityResponseEvent::EnableDelegationEntity(*entity),
            EntityEventMessageAction::DisableDelegation => EntityResponseEvent::EnableDelegationEntity(*entity),
        }
    }
}

impl EntityEventMessage {
    pub fn new_publish<E: Copy + Eq + Hash + Send + Sync>(
        converter: &dyn EntityAndGlobalEntityConverter<E>,
        entity: &E,
    ) -> Self {
        let mut output = Self {
            entity: EntityProperty::new(),
            action: EntityEventMessageAction::Publish,
        };

        output.entity.set(converter, entity);

        output
    }

    pub fn new_unpublish<E: Copy + Eq + Hash + Send + Sync>(
        converter: &dyn EntityAndGlobalEntityConverter<E>,
        entity: &E,
    ) -> Self {
        let mut output = Self {
            entity: EntityProperty::new(),
            action: EntityEventMessageAction::Unpublish,
        };

        output.entity.set(converter, entity);

        output
    }

    pub fn new_enable_delegation<E: Copy + Eq + Hash + Send + Sync>(
        converter: &dyn EntityAndGlobalEntityConverter<E>,
        entity: &E,
    ) -> Self {
        let mut output = Self {
            entity: EntityProperty::new(),
            action: EntityEventMessageAction::EnableDelegation,
        };

        output.entity.set(converter, entity);

        output
    }

    pub fn new_disable_delegation<E: Copy + Eq + Hash + Send + Sync>(
        converter: &dyn EntityAndGlobalEntityConverter<E>,
        entity: &E,
    ) -> Self {
        let mut output = Self {
            entity: EntityProperty::new(),
            action: EntityEventMessageAction::DisableDelegation,
        };

        output.entity.set(converter, entity);

        output
    }
}
