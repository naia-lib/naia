use naia_shared::{Entity, EntityType, LocalEntityKey, StateMask};
use std::{cell::RefCell, rc::Rc};

use super::entity_key::entity_key::EntityKey;

#[derive(Debug)]
pub enum ServerEntityMessage<T: EntityType> {
    CreateEntity(EntityKey, LocalEntityKey, Rc<RefCell<dyn Entity<T>>>),
    UpdateEntity(
        EntityKey,
        LocalEntityKey,
        Rc<RefCell<StateMask>>,
        Rc<RefCell<dyn Entity<T>>>,
    ),
    DeleteEntity(EntityKey, LocalEntityKey),
    AssignPawn(EntityKey, LocalEntityKey),
    UnassignPawn(EntityKey, LocalEntityKey),
}

impl<T: EntityType> ServerEntityMessage<T> {
    pub fn write_message_type(&self) -> u8 {
        match self {
            ServerEntityMessage::CreateEntity(_, _, _) => 0,
            ServerEntityMessage::DeleteEntity(_, _) => 1,
            ServerEntityMessage::UpdateEntity(_, _, _, _) => 2,
            ServerEntityMessage::AssignPawn(_, _) => 3,
            ServerEntityMessage::UnassignPawn(_, _) => 4,
        }
    }
}

impl<T: EntityType> Clone for ServerEntityMessage<T> {
    fn clone(&self) -> Self {
        match self {
            ServerEntityMessage::CreateEntity(gk, lk, e) => {
                ServerEntityMessage::CreateEntity(gk.clone(), lk.clone(), e.clone())
            }
            ServerEntityMessage::DeleteEntity(gk, lk) => {
                ServerEntityMessage::DeleteEntity(gk.clone(), lk.clone())
            }
            ServerEntityMessage::UpdateEntity(gk, lk, sm, e) => {
                ServerEntityMessage::UpdateEntity(gk.clone(), lk.clone(), sm.clone(), e.clone())
            }
            ServerEntityMessage::AssignPawn(gk, lk) => {
                ServerEntityMessage::AssignPawn(gk.clone(), lk.clone())
            }
            ServerEntityMessage::UnassignPawn(gk, lk) => {
                ServerEntityMessage::UnassignPawn(gk.clone(), lk.clone())
            }
        }
    }
}
