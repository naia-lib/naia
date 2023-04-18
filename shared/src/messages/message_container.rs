use std::any::Any;
use std::collections::HashSet;

use naia_serde::BitWrite;

use crate::{LocalEntity, LocalEntityAndGlobalEntityConverter, Message, MessageKind, MessageKinds};

#[derive(Clone)]
pub struct MessageContainer {
    inner: Box<dyn Message>,
    bit_length: u32,
}

impl MessageContainer {
    pub fn from(
        message: Box<dyn Message>,
        converter: &dyn LocalEntityAndGlobalEntityConverter,
    ) -> Self {
        let bit_length = message.bit_length(converter);
        Self {
            inner: message,
            bit_length,
        }
    }

    pub fn name(&self) -> String {
        self.inner.name()
    }

    pub fn bit_length(&self) -> u32 {
        self.bit_length
    }

    pub fn write(
        &self,
        message_kinds: &MessageKinds,
        writer: &mut dyn BitWrite,
        converter: &dyn LocalEntityAndGlobalEntityConverter,
    ) {
        if writer.is_counter() {
            writer.write_bits(self.bit_length);
        } else {
            self.inner.write(message_kinds, writer, converter);
        }
    }

    pub fn is_fragment(&self) -> bool {
        return self.inner.is_fragment();
    }

    pub fn to_boxed_any(self) -> Box<dyn Any> {
        return self.inner.to_boxed_any();
    }

    pub fn kind(&self) -> MessageKind {
        return self.inner.kind();
    }

    pub fn relations_waiting(&self) -> Option<HashSet<LocalEntity>> {
        return self.inner.relations_waiting();
    }

    pub fn relations_complete(&mut self, converter: &dyn LocalEntityAndGlobalEntityConverter) {
        self.inner.relations_complete(converter);
    }
}
