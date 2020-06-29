use crate::EventType;
use std::any::TypeId;

pub trait Event<T: EventType>: EventClone<T> {
    fn is_guaranteed(&self) -> bool;
    fn write(&self, out_bytes: &mut Vec<u8>);
    fn get_typed_copy(&self) -> T;
    fn get_type_id(&self) -> TypeId;
}

pub trait EventClone<T: EventType> {
    fn clone_box(&self) -> Box<dyn Event<T>>;
}

impl<Z: EventType, T: 'static + Event<Z> + Clone> EventClone<Z> for T {
    fn clone_box(&self) -> Box<dyn Event<Z>> {
        Box::new(self.clone())
    }
}

impl<T: EventType> Clone for Box<dyn Event<T>> {
    fn clone(&self) -> Box<dyn Event<T>> {
        EventClone::clone_box(self.as_ref())
    }
}
