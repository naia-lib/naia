use naia_serde::{BitReader, SerdeErr};
use naia_socket_shared::Instant;

use crate::world::local::local_world_manager::LocalWorldManager;
use crate::{
    messages::{
        channels::senders::request_sender::LocalRequestId, message_container::MessageContainer,
        message_kinds::MessageKinds,
    },
    world::remote::remote_entity_waitlist::RemoteEntityWaitlist,
    LocalEntityAndGlobalEntityConverter, LocalResponseId,
};

pub type RequestsAndResponses = (
    Vec<(LocalResponseId, MessageContainer)>,
    Vec<(LocalRequestId, MessageContainer)>,
);

/// Trait implemented by all channel receivers that surface typed payloads.
pub trait ChannelReceiver<P>: Send + Sync {
    /// Read messages from an internal buffer and return their content
    fn receive_messages(
        &mut self,
        message_kinds: &MessageKinds,
        now: &Instant,
        entity_waitlist: &mut RemoteEntityWaitlist,
        converter: &dyn LocalEntityAndGlobalEntityConverter,
    ) -> Vec<P>;
}

/// Extended receiver trait for message channels that also parses raw wire bits and surfaces request/response pairs.
pub trait MessageChannelReceiver: ChannelReceiver<MessageContainer> {
    /// Read messages from raw bits, parse them and store then into an internal buffer
    fn read_messages(
        &mut self,
        message_kinds: &MessageKinds,
        local_world_manager: &mut LocalWorldManager,
        reader: &mut BitReader,
    ) -> Result<(), SerdeErr>;

    /// Drains and returns all pending request/response pairs from this channel's internal buffer.
    fn receive_requests_and_responses(
        &mut self,
    ) -> RequestsAndResponses;
}
