use std::{
    any::{TypeId},
    rc::Rc,
    cell::RefCell,
};

use crate::{EntityType, StateMask, EntityMutator};

pub trait NetEntity<T: EntityType>: NetEntityType<T> {
    fn get_state_mask_size(&self) -> u8;
    fn to_type(&self) -> T;
    fn write(&self, out_bytes: &mut Vec<u8>);
    fn write_partial(&self, state_mask: &Rc<RefCell<StateMask>>, out_bytes: &mut Vec<u8>);
    fn read(&mut self, in_bytes: &[u8]);
    fn read_partial(&mut self, state_mask: &StateMask, in_bytes: &[u8]);
    fn print(&self, key: u16);
    fn set_mutator(&mut self, mutator: &Rc<RefCell<dyn EntityMutator>>);
}

pub trait NetEntityType<T: EntityType> {
    fn get_type_id(&self) -> TypeId;
}

impl<Z: EntityType, T: 'static + NetEntity<Z>> NetEntityType<Z> for T {
    fn get_type_id(&self) -> TypeId { return TypeId::of::<T>(); }
}