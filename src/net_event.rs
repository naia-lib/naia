
use super::net_base::NetBase;
use crate::ManifestType;
use std::borrow::Borrow;

pub trait NetEvent<T: ManifestType>: NetBase<T> + NetEventClone<T> {
//    fn is_guaranteed() -> bool;
    fn write(&self, out_bytes: &mut Vec<u8>);
    fn read(&mut self, in_bytes: &[u8]);
}

pub trait NetEventClone<T: ManifestType> {
    fn clone_box(&self) -> Box<dyn NetEvent<T>>;
}

impl<Z: ManifestType, T: 'static + NetEvent<Z> + Clone> NetEventClone<Z> for T {
    fn clone_box(&self) -> Box<dyn NetEvent<Z>> {
        Box::new(self.clone())
    }
}

impl<T: ManifestType> Clone for Box<dyn NetEvent<T>> {
    fn clone(&self) -> Box<dyn NetEvent<T>> {
        NetEventClone::clone_box(self.as_ref())
    }
}