use crate::{EntityKey, NetEntity, EntityType, StateMask};
use std::{
    rc::Rc,
    cell::RefCell
};

#[derive(Clone)]
pub enum ServerEntityMessage<T: EntityType> {
    Create(EntityKey, u16, Rc<RefCell<dyn NetEntity<T>>>),
    Update(EntityKey, u16, Rc<RefCell<StateMask>>, Rc<RefCell<dyn NetEntity<T>>>),
    Delete(EntityKey, u16),
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