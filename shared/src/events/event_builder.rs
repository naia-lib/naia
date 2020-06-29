use crate::EventType;
use std::any::TypeId;

pub trait EventBuilder<T: EventType> {
    fn get_type_id(&self) -> TypeId;
    fn build(&self, in_bytes: &[u8]) -> T;
}
