use std::{
    rc::Rc,
};
use gaia_shared::{LocalEntityKey};

pub enum ClientEvent<T, U> {
    Connection,
    Disconnection,
    Event(T),
    CreateEntity(LocalEntityKey, Rc<U>),
    UpdateEntity(LocalEntityKey),
    DeleteEntity(LocalEntityKey),
    None,
}