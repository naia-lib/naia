use crate::EntityType;
use std::any::TypeId;

pub trait EntityBuilder<T: EntityType> {
    fn build(&self, in_bytes: &[u8]) -> T;
    fn get_type_id(&self) -> TypeId;
}
