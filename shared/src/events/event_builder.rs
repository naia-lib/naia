use crate::EventType;
use std::{
    any::TypeId,
    fmt::{Debug, Formatter, Result},
};

pub trait EventBuilder<T: EventType> {
    fn get_type_id(&self) -> TypeId;
    fn build(&self, in_bytes: &[u8]) -> T;
}

impl<T: EventType> Debug for Box<dyn EventBuilder<T>> {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        f.write_str("Boxed EventBuilder")
    }
}
