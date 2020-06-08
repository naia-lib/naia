use crate::{LocalEntityKey, NetEntity, EntityType};
use std::{
    rc::Rc,
    cell::RefCell
};

#[derive(Clone)]
pub enum EntityMessage<T: EntityType> {
    Create(LocalEntityKey, Rc<RefCell<dyn NetEntity<T>>>),
    Update(LocalEntityKey),
    Delete(LocalEntityKey),
}