use std::{any::TypeId, cell::RefCell, rc::Rc};

use crate::{EntityMutator, EntityType, StateMask};

pub trait Entity<T: EntityType> {
    fn get_state_mask_size(&self) -> u8;
    fn get_typed_copy(&self) -> T;
    fn get_type_id(&self) -> TypeId;
    fn write(&self, out_bytes: &mut Vec<u8>);
    fn write_partial(&self, state_mask: &StateMask, out_bytes: &mut Vec<u8>);
    fn read_partial(&mut self, state_mask: &StateMask, in_bytes: &[u8]);
    fn set_mutator(&mut self, mutator: &Rc<RefCell<dyn EntityMutator>>);
}
