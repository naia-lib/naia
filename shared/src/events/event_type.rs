use std::any::TypeId;

pub trait EventType: Clone {
    //write & get_type_id are ONLY currently used for reading/writing auth events.. maybe should do something different here
    fn write(&mut self, buffer: &mut Vec<u8>);
    fn get_type_id(&self) -> TypeId;
}
