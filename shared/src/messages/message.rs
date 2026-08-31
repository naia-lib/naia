use std::{any::Any, collections::HashSet};

use naia_serde::{BitReader, BitWrite, SerdeErr};

use crate::{
    messages::message_kinds::{MessageKind, MessageKinds},
    named::Named,
    world::entity::entity_converters::LocalEntityAndGlobalEntityConverterMut,
    LocalEntityAndGlobalEntityConverter, MessageContainer, RemoteEntity,
};

/// Factory trait that deserializes a concrete `Message` from raw bits.
pub trait MessageBuilder: Send + Sync {
    /// Create new Message from incoming bit stream
    fn read(
        &self,
        reader: &mut BitReader,
        converter: &dyn LocalEntityAndGlobalEntityConverter,
    ) -> Result<MessageContainer, SerdeErr>;

    /// Returns a heap-allocated clone of this builder.
    fn box_clone(&self) -> Box<dyn MessageBuilder>;
}

/// Core trait for all naia message types — provides serialization, kind lookup, and entity-relation hooks.
pub trait Message: Send + Sync + Named + MessageClone + Any {
    /// Gets the MessageKind of this type
    fn kind(&self) -> MessageKind;
    /// Converts this boxed message into a `Box<dyn Any>` for downcasting.
    fn to_boxed_any(self: Box<Self>) -> Box<dyn Any>;
    /// Creates the `MessageBuilder` used to deserialize instances of this type.
    fn create_builder() -> Box<dyn MessageBuilder>
    where
        Self: Sized;
    /// Returns the bit length of this message when serialized with `converter`.
    fn bit_length(
        &self,
        message_kinds: &MessageKinds,
        converter: &mut dyn LocalEntityAndGlobalEntityConverterMut,
    ) -> u32;
    /// Returns `true` if this message is a fragment of a larger logical message.
    fn is_fragment(&self) -> bool;
    /// Returns `true` if this message envelope carries a request or response payload.
    fn is_request(&self) -> bool;
    /// Writes data into an outgoing byte stream
    fn write(
        &self,
        message_kinds: &MessageKinds,
        writer: &mut dyn BitWrite,
        converter: &mut dyn LocalEntityAndGlobalEntityConverterMut,
    );
    /// Returns a list of RemoteEntities contained within the Message's EntityProperty fields, which have not yet been received.
    fn relations_waiting(&self) -> Option<HashSet<RemoteEntity>>;
    /// Converts any LocalEntities contained within the Message's EntityProperty fields to GlobalEntities
    fn relations_complete(&mut self, converter: &dyn LocalEntityAndGlobalEntityConverter);
    // /// Returns whether has any EntityRelations
    // fn has_entity_relations(&self) -> bool;
    // /// Returns a list of Entities contained within the Message's EntityRelation fields
    // fn entities(&self) -> Vec<GlobalEntity>;
}

// Named
impl Named for Box<dyn Message> {
    fn name(&self) -> String {
        self.as_ref().name()
    }

    fn protocol_name() -> &'static str
    where
        Self: Sized,
    {
        // This is unreachable since Box<dyn Message> is not Sized
        unimplemented!("protocol_name() is not available for Box<dyn Message>")
    }
}

/// Helper trait enabling `Box<dyn Message>` to be cloned without knowing the concrete type.
pub trait MessageClone {
    /// Returns a heap-allocated clone of `self` as a `Box<dyn Message>`.
    fn clone_box(&self) -> Box<dyn Message>;
}

impl<T: 'static + Clone + Message> MessageClone for T {
    fn clone_box(&self) -> Box<dyn Message> {
        Box::new(self.clone())
    }
}

impl Clone for Box<dyn Message> {
    fn clone(&self) -> Box<dyn Message> {
        MessageClone::clone_box(self.as_ref())
    }
}

#[cfg(test)]
mod message_tests {
    use crate::{Message, MessageKind, Named};

    #[derive(Message)]
    struct Whisper {
        value: u8,
    }

    #[test]
    fn a_boxed_message_reports_the_concrete_types_name() {
        let boxed: Box<dyn Message> = Box::new(Whisper { value: 1 });

        assert_eq!(boxed.name(), "Whisper".to_string());
        assert_eq!(Whisper::protocol_name(), "Whisper");
    }

    #[test]
    #[should_panic(expected = "protocol_name() is not available for Box<dyn Message>")]
    fn a_boxed_message_has_no_protocol_name_of_its_own() {
        // Unreachable in practice -- `Box<dyn Message>` is not `Sized`, so the
        // trait's `where Self: Sized` bound keeps callers off it. Pinned so the
        // arm stays a loud failure rather than silently returning something.
        <Box<dyn Message> as Named>::protocol_name();
    }

    #[test]
    fn cloning_a_boxed_message_copies_the_value_behind_the_trait_object() {
        let original: Box<dyn Message> = Box::new(Whisper { value: 42 });
        let copy = original.clone();

        assert_eq!(copy.kind(), MessageKind::of::<Whisper>());
        let recovered = copy
            .to_boxed_any()
            .downcast::<Whisper>()
            .expect("clone must preserve the concrete type");
        assert_eq!(recovered.value, 42);
    }
}
