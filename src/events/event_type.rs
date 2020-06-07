pub trait EventType: Clone {
    fn read(&mut self, bytes: &[u8]);
}