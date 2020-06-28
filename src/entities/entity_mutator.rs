pub trait EntityMutator {
    fn mutate(&mut self, property_index: u8);
}
