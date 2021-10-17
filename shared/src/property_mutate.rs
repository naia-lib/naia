
/// Tracks which Properties have changed and need to be queued for syncing with
/// the Client
pub trait PropertyMutate: Send + Sync + 'static {
    /// Given the index of the Property whose value has changed, queue that
    /// Property for transmission to the Client
    fn mutate(&mut self, property_index: u8);
}

pub struct PropertyMutator {
    inner: Box<dyn PropertyMutate>,
}

impl PropertyMutator {
    pub fn new<M: PropertyMutate>(mutator: M) -> Self {
        let inner = Box::new(mutator);
        return Self {
            inner
        }
    }
}