use std::{any::Any, collections::HashSet};

use naia_serde::BitWrite;

use crate::{
    world::entity::{
        entity_converters::LocalEntityAndGlobalEntityConverterMut, local_entity::RemoteEntity,
    },
    LocalEntityAndGlobalEntityConverter, Message, MessageKind, MessageKinds,
};

#[derive(Clone)]
pub struct MessageContainer {
    inner: Box<dyn Message>,
    bit_length: Option<u32>,
}

impl MessageContainer {
    pub fn from_write(
        message: Box<dyn Message>,
        converter: &mut dyn LocalEntityAndGlobalEntityConverterMut,
    ) -> Self {
        let bit_length = message.bit_length(converter);
        Self {
            inner: message,
            bit_length: Some(bit_length),
        }
    }

    pub fn from_read(message: Box<dyn Message>) -> Self {
        Self {
            inner: message,
            bit_length: None,
        }
    }

    pub fn name(&self) -> String {
        self.inner.name()
    }

    pub fn bit_length(&self) -> u32 {
        self.bit_length.expect("bit_length should never be called on a MessageContainer that was created from a read operation")
    }

    pub fn write(
        &self,
        message_kinds: &MessageKinds,
        writer: &mut dyn BitWrite,
        converter: &mut dyn LocalEntityAndGlobalEntityConverterMut,
    ) {
        if writer.is_counter() {
            writer.count_bits(self.bit_length());
        } else {
            self.inner.write(message_kinds, writer, converter);
        }
    }

    pub fn is_fragment(&self) -> bool {
        return self.inner.is_fragment();
    }

    pub fn is_request_or_response(&self) -> bool {
        return self.inner.is_request();
    }

    pub fn to_boxed_any(self) -> Box<dyn Any> {
        return self.inner.to_boxed_any();
    }

    pub fn kind(&self) -> MessageKind {
        return self.inner.kind();
    }

    pub fn relations_waiting(&self) -> Option<HashSet<RemoteEntity>> {
        return self.inner.relations_waiting();
    }

    pub fn relations_complete(&mut self, converter: &dyn LocalEntityAndGlobalEntityConverter) {
        self.inner.relations_complete(converter);
    }
}
