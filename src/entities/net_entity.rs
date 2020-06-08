use std::any::{TypeId};
use crate::{EntityType};

pub trait NetEntity<T: EntityType>: NetEntityType<T> {
    fn get_state_mask_size(&self) -> u8;
    fn to_type(&self) -> T;
//    fn write_create(&self, out_bytes: &mut Vec<u8>);
//    fn write_update(&self, out_bytes: &mut Vec<u8>);
    fn read(&mut self, in_bytes: &[u8]);


//    fn read_update(in_bytes: &mut [u8]) -> Self;
//    fn disappear(&self);
//    fn delete(&self);
}

pub trait NetEntityType<T: EntityType> {
    fn get_type_id(&self) -> TypeId;
}

impl<Z: EntityType, T: 'static + NetEntity<Z>> NetEntityType<Z> for T {
    fn get_type_id(&self) -> TypeId { return TypeId::of::<T>(); }
}