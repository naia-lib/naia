use naia_derive::MessageInternal;
use naia_serde::SerdeInternal;

use crate::GlobalEntity;

#[derive(MessageInternal)]
pub struct EntityAuthEvent {
    pub inner: EntityAuthEventInner,
}

#[derive(SerdeInternal, Clone, Debug, PartialEq)]
pub enum EntityAuthEventInner {
    Publish(GlobalEntity),
}

impl EntityAuthEvent {
    pub fn new_publish(global_entity: &GlobalEntity) -> Self {
        Self {
            inner: EntityAuthEventInner::Publish(*global_entity),
        }
    }
}
