use std::any::{Any, TypeId};
use crate::ManifestType;

pub trait NetBase<T: ManifestType>: Any + NetBaseClone<T> + NetBaseType<T> {
    fn to_type(&self) -> T;
    fn is_event(&self) -> bool;
}

pub trait NetBaseClone<T: ManifestType> {
    fn clone_box(&self) -> Box<dyn NetBase<T>>;
}

pub trait NetBaseType<T: ManifestType> {
    fn get_type_id(&self) -> TypeId;
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

impl<Z: ManifestType, T: NetBase<Z>> NetBaseType<Z> for T {
    fn get_type_id(&self) -> TypeId { return TypeId::of::<T>(); }
}