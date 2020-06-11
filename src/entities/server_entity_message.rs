use crate::{EntityKey, LocalEntityKey, NetEntity, EntityType, StateMask};
use std::{
    rc::Rc,
    cell::RefCell
};

#[derive(Clone)]
pub enum ServerEntityMessage<T: EntityType> {
    Create(EntityKey, LocalEntityKey, Rc<RefCell<dyn NetEntity<T>>>),
    Update(EntityKey, LocalEntityKey, Rc<RefCell<StateMask>>, Rc<RefCell<dyn NetEntity<T>>>),
    Delete(EntityKey, LocalEntityKey),
}

impl<T: EntityType> ServerEntityMessage<T> {
    pub fn write_message_type(&self) -> u8 {
        match self {
            ServerEntityMessage::Create(_, _, _) => 0,
            ServerEntityMessage::Delete(_, _) => 1,
            ServerEntityMessage::Update(_, _, _, _) => 2,
        }
    }
}