use naia_serde::{BitReader, SerdeErr};

use crate::{
    messages::{message_container::MessageContainer, message_kinds::MessageKinds},
    LocalEntityAndGlobalEntityConverter,
};

pub trait ChannelReceiver<P>: Send + Sync {
    /// Read messages from an internal buffer and return their content
    fn receive_messages(&mut self) -> Vec<P>;
}

pub trait MessageChannelReceiver: ChannelReceiver<MessageContainer> {
    /// Read messages from raw bits, parse them and store then into an internal buffer
    fn read_messages(
        &mut self,
        message_kinds: &MessageKinds,
        converter: &dyn LocalEntityAndGlobalEntityConverter,
        reader: &mut BitReader,
    ) -> Result<(), SerdeErr>;
}
