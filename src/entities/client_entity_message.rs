use crate::{LocalEntityKey, EntityType};

#[derive(Clone)]
pub enum ClientEntityMessage<T: EntityType> {
    Create(LocalEntityKey, T),
    Update(LocalEntityKey),
    Delete(LocalEntityKey),
}