use std::{any::Any, collections::HashSet, sync::Arc};

use naia_serde::BitWrite;

use crate::world::local::local_entity::RemoteEntity;
use crate::{
    world::entity::entity_converters::LocalEntityAndGlobalEntityConverterMut,
    LocalEntityAndGlobalEntityConverter, Message, MessageKind, MessageKinds,
};

/// A reference-counted wrapper around a heap-allocated [`Message`] trait object.
///
/// ## Why `Arc<Box<dyn Message>>`?
///
/// `broadcast_message` and `room_broadcast_message` send the same logical
/// message to every connected user. With a plain `Box<dyn Message>` this
/// required one `clone_box()` call (heap allocation + copy) per user. At
/// 1,262 CCU that is 1,262 allocations per broadcast tick.
///
/// Wrapping in `Arc` makes `clone()` a single atomic refcount increment
/// regardless of how many users share the message. Each connection still
/// serialises the message independently through its own entity converter —
/// the shared data is immutable (only `&self` methods called on the send path).
///
/// `to_boxed_any` (receive path only) extracts the inner `Box<dyn Message>`
/// via `Arc::try_unwrap`; in the rare case the Arc is still shared it falls
/// back to `clone_box()`, preserving correctness without unsafe code.
#[derive(Clone)]
pub struct MessageContainer {
    inner: Arc<Box<dyn Message>>,
}

impl MessageContainer {
    /// Wraps `message` in an `Arc` so it can be cheaply shared across broadcast targets.
    pub fn new(message: Box<dyn Message>) -> Self {
        Self {
            inner: Arc::new(message),
        }
    }

    /// Returns the protocol name of the contained message type.
    pub fn name(&self) -> String {
        self.inner.name()
    }

    /// Returns the serialized bit length of this message given `converter` for entity references.
    pub fn bit_length(
        &self,
        message_kinds: &MessageKinds,
        converter: &mut dyn LocalEntityAndGlobalEntityConverterMut,
    ) -> u32 {
        self.inner.bit_length(message_kinds, converter)
    }

    /// Writes the message's kind tag and payload bits into `writer`.
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

    /// Returns `true` if this message is a fragment of a larger logical message.
    pub fn is_fragment(&self) -> bool {
        self.inner.is_fragment()
    }

    /// Returns `true` if this message envelope carries a request or response (not a plain message).
    pub fn is_request_or_response(&self) -> bool {
        self.inner.is_request()
    }

    /// Converts this container into a `Box<dyn Any>` for downcasting to the concrete message type.
    pub fn to_boxed_any(self) -> Box<dyn Any> {
        // Fast path: if this is the only Arc reference (always true after the
        // message is dequeued from a connection's send buffer), extract without
        // allocating. Fallback clones only in the pathological case where a
        // broadcast Arc is still live when to_boxed_any is called — not expected
        // in practice but required for correctness.
        match Arc::try_unwrap(self.inner) {
            Ok(boxed) => boxed.to_boxed_any(),
            Err(arc) => (*arc).clone_box().to_boxed_any(),
        }
    }

    /// Returns the `MessageKind` identifying the concrete message type inside this container.
    pub fn kind(&self) -> MessageKind {
        self.inner.kind()
    }

    /// Returns the set of remote entities this message is still waiting on, or `None` if ready.
    pub fn relations_waiting(&self) -> Option<HashSet<RemoteEntity>> {
        self.inner.relations_waiting()
    }

    /// Notifies the inner message that all awaited entity relations have been resolved.
    pub fn relations_complete(&mut self, converter: &dyn LocalEntityAndGlobalEntityConverter) {
        // relations_complete requires &mut self on the inner message.
        // Since we hold an Arc, we must have exclusive ownership to mutate.
        // This is only called on the receive path where no other Arc clones
        // are live, so make_mut gives us a unique clone if needed (which is
        // already a Box<dyn Message> clone — same cost as before this change).
        Arc::make_mut(&mut self.inner).relations_complete(converter);
    }
}
