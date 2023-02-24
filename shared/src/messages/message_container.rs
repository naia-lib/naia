use naia_serde::BitWrite;

use crate::{Message, MessageKinds, NetEntityHandleConverter};

#[derive(Clone)]
pub struct MessageContainer {
    inner: Box<dyn Message>,
    bit_length: u32,
}

impl MessageContainer {

    pub fn from(message: Box<dyn Message>) -> Self {
        let bit_length = message.bit_length();
        Self {
            bit_length,
            inner: message,
        }
    }

    pub fn name(&self) -> String {
        self.inner.name()
    }

    pub fn write(
        &self,
        message_kinds: &MessageKinds,
        writer: &mut dyn BitWrite,
        converter: &dyn NetEntityHandleConverter,
    ) {
        self.inner.write(message_kinds, writer, converter);
    }
}