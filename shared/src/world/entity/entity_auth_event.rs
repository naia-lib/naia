use std::hash::Hash;

use naia_derive::MessageInternal;
use naia_serde::SerdeInternal;

use crate::{EntityAndGlobalEntityConverter, EntityProperty};

#[derive(MessageInternal)]
pub struct EntityEventMessage {
    pub entity: EntityProperty,
    pub action: EntityEventMessageAction,
}

#[derive(SerdeInternal, Clone, Debug, PartialEq)]
pub enum EntityEventMessageAction {
    Publish,
    Unpublish,
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
}
