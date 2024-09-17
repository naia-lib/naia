use naia_shared::{Message, Serde};

#[derive(Message)]
pub struct StringMessage<T: Send + Sync + 'static + Serde> {
    pub contents: String,
    something: T,
}

impl<T: Send + Sync + 'static + Serde> StringMessage<T> {
    pub fn new(contents: String, something: T) -> Self {
        Self {
            contents,
            something,
        }
    }
}
