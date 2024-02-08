
use naia_shared::Message;

#[derive(Message)]
pub struct StringMessage<T: Send + Sync + 'static> {
    pub contents: String,
    phantom_t: std::marker::PhantomData<T>,
}

impl<T: Send + Sync> StringMessage<T> {
    pub fn new(contents: String) -> Self {
        Self { contents, phantom_t: std::marker::PhantomData }
    }
}
