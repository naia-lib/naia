use std::any::{TypeId};
use crate::EventType;

pub trait Event<T: EventType>: EventTypeGetter<T> + EventClone<T> {
    fn is_guaranteed(&self) -> bool;
    fn to_type(&self) -> T;
    fn write(&self, out_bytes: &mut Vec<u8>);
    fn read(&mut self, in_bytes: &[u8]);
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

pub trait EventTypeGetter<T: EventType> {
    fn get_type_id(&self) -> TypeId;
}

impl<Z: EventType, T: 'static + Event<Z>> EventTypeGetter<Z> for T {
    fn get_type_id(&self) -> TypeId { return TypeId::of::<T>(); }
}