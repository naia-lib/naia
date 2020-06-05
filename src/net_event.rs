use std::any::{TypeId};
use crate::EventType;

pub trait NetEvent<T: EventType>: NetEventType<T> + NetEventClone<T> {
    fn is_guaranteed(&self) -> bool;
    fn to_type(&self) -> T;
    fn write(&self, out_bytes: &mut Vec<u8>);
    fn read(&mut self, in_bytes: &[u8]);
}

pub trait NetEventClone<T: EventType> {
    fn clone_box(&self) -> Box<dyn NetEvent<T>>;
}

impl<Z: EventType, T: 'static + NetEvent<Z> + Clone> NetEventClone<Z> for T {
    fn clone_box(&self) -> Box<dyn NetEvent<Z>> {
        Box::new(self.clone())
    }
}

impl<T: EventType> Clone for Box<dyn NetEvent<T>> {
    fn clone(&self) -> Box<dyn NetEvent<T>> {
        NetEventClone::clone_box(self.as_ref())
    }
}

pub trait NetEventType<T: EventType> {
    fn get_type_id(&self) -> TypeId;
}


impl<Z: EventType, T: 'static + NetEvent<Z>> NetEventType<Z> for T {
    fn get_type_id(&self) -> TypeId { return TypeId::of::<T>(); }
}