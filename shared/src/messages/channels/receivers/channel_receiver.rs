use naia_serde::{BitReader, SerdeErr};
use naia_socket_shared::Instant;

use crate::messages::channels::senders::request_sender::LocalRequestId;
use crate::{
    messages::{message_container::MessageContainer, message_kinds::MessageKinds},
    world::remote::entity_waitlist::EntityWaitlist,
    LocalEntityAndGlobalEntityConverter, LocalResponseId,
};

pub trait ChannelReceiver<P>: Send + Sync {
    /// Read messages from an internal buffer and return their content
    fn receive_messages(
        &mut self,
        message_kinds: &MessageKinds,
        now: &Instant,
        entity_waitlist: &mut EntityWaitlist,
        converter: &dyn LocalEntityAndGlobalEntityConverter,
    ) -> Vec<P>;
}

pub trait MessageChannelReceiver: ChannelReceiver<MessageContainer> {
    /// Read messages from raw bits, parse them and store then into an internal buffer
    fn read_messages(
        &mut self,
        message_kinds: &MessageKinds,
        entity_waitlist: &mut EntityWaitlist,
        converter: &dyn LocalEntityAndGlobalEntityConverter,
        reader: &mut BitReader,
    ) -> Result<(), SerdeErr>;

    fn receive_requests_and_responses(
        &mut self,
    ) -> (
        Vec<(LocalResponseId, MessageContainer)>,
        Vec<(LocalRequestId, MessageContainer)>,
    );
}
