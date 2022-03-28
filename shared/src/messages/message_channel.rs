use naia_serde::{BitReader, BitWriter};

use crate::{
    protocol::protocolize::Protocolize, types::MessageId, NetEntityHandleConverter,
};

pub trait ChannelSender<P: Protocolize> {
    fn send_message(&mut self, message: P);
    fn collect_messages(&mut self, rtt_millis: &f32);
    fn has_messages(&self) -> bool;
    fn write_messages(
        &mut self,
        converter: &dyn NetEntityHandleConverter,
        writer: &mut BitWriter,
    ) -> Option<Vec<MessageId>>;
    fn notify_message_delivered(&mut self, message_id: &MessageId);
}

pub trait ChannelReceiver<P: Protocolize> {
    fn read_messages(
        &mut self,
        reader: &mut BitReader,
        converter: &dyn NetEntityHandleConverter,
    );
    fn receive_messages(&mut self) -> Vec<P>;
}
