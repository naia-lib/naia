use gaia_shared::{LocalEntityKey, NetEntity, EntityType, StateMask};
use std::{
    rc::Rc,
    cell::RefCell
};

use super::{
    entity_key::EntityKey,
    //server_entity::ServerEntity,
};

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

impl<T: EntityType> Clone for ServerEntityMessage<T> {
    fn clone(&self) -> Self {
        match self {
            ServerEntityMessage::Create(gk, lk, e) =>
                ServerEntityMessage::Create(gk.clone(), lk.clone(), e.clone()),
            ServerEntityMessage::Delete(gk, lk) =>
                ServerEntityMessage::Delete(gk.clone(), lk.clone()),
            ServerEntityMessage::Update(gk, lk, sm, e) =>
                ServerEntityMessage::Update(gk.clone(), lk.clone(), sm.clone(), e.clone()),
        }
    }
}