use crate::{EventType};
use std::any::{TypeId};

pub trait EventBuilder<T: EventType> {
    fn build(&self, in_bytes: &[u8]) -> T;
    fn get_type_id(&self) -> TypeId;
}
//
//pub trait EventBuilderTypeGetter<T: EventType> {
//    fn get_type_id(&self) -> TypeId;
//}
//
//impl<T: EventType> EventBuilderTypeGetter<T> for Box<dyn EventBuilder<T>> {
//    fn get_type_id(&self) -> TypeId { return TypeId::of::<Event<T>>(); }
//}