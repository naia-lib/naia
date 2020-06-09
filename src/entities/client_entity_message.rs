use crate::{LocalEntityKey, EntityKey, NetEntity, EntityType};
use std::{
    rc::Rc,
    cell::RefCell,
};

#[derive(Clone)]
pub enum ClientEntityMessage<T: EntityType> {
    Create(LocalEntityKey, T),
    Update(LocalEntityKey),
    Delete(LocalEntityKey),
}