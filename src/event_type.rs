pub trait EventType {
    fn optional_clone(&self) -> Option<Self> where Self: Sized;
    fn use_bytes(&mut self, bytes: &[u8]);
}