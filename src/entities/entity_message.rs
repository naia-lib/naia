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

impl<T: EntityType> EntityMessage<T> {
    pub fn write_message_type(&self) -> u8 {
        match self {
            EntityMessage::Create(_, _, _) => 0,
            EntityMessage::Update(_, _) => 1,
            EntityMessage::Delete(_, _) => 2,
        }
    }
}