use crate::EntityType;
use std::{
    fmt::{
        Formatter,
        Debug,
        Result,
    },
    any::TypeId
};

pub trait EntityBuilder<T: EntityType> {
    fn build(&self, in_bytes: &[u8]) -> T;
    fn get_type_id(&self) -> TypeId;
}

impl<T: EntityType> Debug for Box<dyn EntityBuilder<T>> {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        f.write_str("Boxed EntityBuilder")
    }
}