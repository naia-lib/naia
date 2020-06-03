use std::any::Any;
use crate::ManifestType;

pub trait NetBase<T: ManifestType>: Any + NetBaseClone<T> {
    fn to_type(self) -> T;
    fn is_event(&self) -> bool;
}

pub trait NetBaseClone<T: ManifestType> {
    fn clone_box(&self) -> Box<dyn NetBase<T>>;
}

impl<Z: ManifestType, T: 'static + NetBase<Z> + Clone> NetBaseClone<Z> for T {
    fn clone_box(&self) -> Box<dyn NetBase<Z>> {
        Box::new(self.clone())
    }
}

impl<T: ManifestType> Clone for Box<dyn NetBase<T>> {
    fn clone(&self) -> Box<dyn NetBase<T>> {
        self.clone_box()
    }
}