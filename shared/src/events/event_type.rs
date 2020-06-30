use std::any::TypeId;

/// An Enum with a variant for every Event that can be sent to a remote host
pub trait EventType: Clone {
    // write & get_type_id are ONLY currently used for reading/writing auth events..
    // maybe should do something different here
    /// Writes the typed Event into an outgoing byte stream
    fn write(&mut self, buffer: &mut Vec<u8>);
    /// Get the TypeId of the contained Event
    fn get_type_id(&self) -> TypeId;
}
