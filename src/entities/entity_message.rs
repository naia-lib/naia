use crate::{LocalEntityKey, EntityKey, NetEntity, EntityType};
use std::{
    rc::Rc,
    cell::RefCell
};

#[derive(Clone)]
pub enum EntityMessage<T: EntityType> {
    Create(EntityKey, LocalEntityKey, Rc<RefCell<dyn NetEntity<T>>>),
    Update(EntityKey, LocalEntityKey),
    Delete(EntityKey, LocalEntityKey),
}