use std::{any::Any, collections::HashSet};

use naia_serde::BitWrite;

use crate::world::local::local_entity::RemoteEntity;
use crate::{
    world::entity::entity_converters::LocalEntityAndGlobalEntityConverterMut,
    LocalEntityAndGlobalEntityConverter, Message, MessageKind, MessageKinds,
};

#[derive(Clone)]
pub struct MessageContainer {
    inner: Box<dyn Message>,
}

impl MessageContainer {
    pub fn new(message: Box<dyn Message>) -> Self {
        Self { inner: message }
    }

    pub fn name(&self) -> String {
        self.inner.name()
    }

    pub fn bit_length(
        &self,
        message_kinds: &MessageKinds,
        converter: &mut dyn LocalEntityAndGlobalEntityConverterMut,
    ) -> u32 {
        self.inner.bit_length(message_kinds, converter)
    }

    pub fn write(
        &self,
        message_kinds: &MessageKinds,
        writer: &mut dyn BitWrite,
        converter: &mut dyn LocalEntityAndGlobalEntityConverterMut,
    ) {
        // Counter mode and real-write mode share the same path: every
        // `write_bit` against a `BitCounter` is a no-op-write-increment, so
        // the inner traversal counts bits correctly without a separate
        // bit_length() round-trip.
        self.inner.write(message_kinds, writer, converter);
    }

    pub fn is_fragment(&self) -> bool {
        self.inner.is_fragment()
    }

    pub fn is_request_or_response(&self) -> bool {
        self.inner.is_request()
    }

    pub fn to_boxed_any(self) -> Box<dyn Any> {
        self.inner.to_boxed_any()
    }

    pub fn kind(&self) -> MessageKind {
        self.inner.kind()
    }

    pub fn relations_waiting(&self) -> Option<HashSet<RemoteEntity>> {
        self.inner.relations_waiting()
    }

    pub fn relations_complete(&mut self, converter: &dyn LocalEntityAndGlobalEntityConverter) {
        self.inner.relations_complete(converter);
    }
}
