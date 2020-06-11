use crate::{EntityKey, NetEntity, EntityType};
use std::{
    rc::Rc,
    cell::RefCell,
};

#[derive(Clone)]
pub enum ClientEntityMessage<T: EntityType> {
    Create(u16, T),
    Update(u16),
    Delete(u16),
}