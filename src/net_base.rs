use std::any::Any;
pub trait NetBase<T>: Any {
    fn to_type(self) -> T;
    fn is_event(&self) -> bool;
}