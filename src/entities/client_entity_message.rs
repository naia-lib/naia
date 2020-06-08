use crate::{LocalEntityKey, EntityKey, NetEntity, EntityType};
use std::{
    rc::Rc,
};

#[derive(Clone)]
pub enum ClientEntityMessage<T: EntityType> {
    Create(LocalEntityKey, Rc<T>),
    Update(LocalEntityKey),
    Delete(LocalEntityKey),
}