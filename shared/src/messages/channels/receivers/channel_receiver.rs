use naia_serde::{BitReader, SerdeErr};

use crate::{LocalEntityAndGlobalEntityConverter, MessageKind, messages::{message_container::MessageContainer, message_kinds::MessageKinds}, world::remote::entity_waitlist::EntityWaitlist};
use crate::messages::channels::senders::request_sender::LocalRequestId;

pub trait ChannelReceiver<P>: Send + Sync {
    /// Read messages from an internal buffer and return their content
    fn receive_messages(
        &mut self,
        message_kinds: &MessageKinds,
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

    fn receive_requests(&mut self) -> Vec<(MessageKind, LocalRequestId, MessageContainer)>;
}
