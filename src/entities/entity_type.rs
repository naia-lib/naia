pub trait EntityType: Clone {
    fn read(&mut self, bytes: &[u8]);
}