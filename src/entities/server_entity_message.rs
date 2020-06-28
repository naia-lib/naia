use naia_shared::{Entity, EntityType, LocalEntityKey, StateMask};
use std::{cell::RefCell, rc::Rc};

use super::entity_key::EntityKey;

pub enum ServerEntityMessage<T: EntityType> {
    Create(EntityKey, LocalEntityKey, Rc<RefCell<dyn Entity<T>>>),
    Update(
        EntityKey,
        LocalEntityKey,
        Rc<RefCell<StateMask>>,
        Rc<RefCell<dyn Entity<T>>>,
    ),
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

impl<T: EntityType> Clone for ServerEntityMessage<T> {
    fn clone(&self) -> Self {
        match self {
            ServerEntityMessage::Create(gk, lk, e) => {
                ServerEntityMessage::Create(gk.clone(), lk.clone(), e.clone())
            }
            ServerEntityMessage::Delete(gk, lk) => {
                ServerEntityMessage::Delete(gk.clone(), lk.clone())
            }
            ServerEntityMessage::Update(gk, lk, sm, e) => {
                ServerEntityMessage::Update(gk.clone(), lk.clone(), sm.clone(), e.clone())
            }
        }
    }
}
