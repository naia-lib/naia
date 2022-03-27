use naia_serde::{BitReader, BitWriter};

use crate::{
    protocol::protocolize::Protocolize, types::MessageId, Manifest,
    NetEntityHandleConverter,
};

pub trait MessageChannel<P: Protocolize> {
    fn send_message(&mut self, message: P);
    fn collect_outgoing_messages(&mut self, rtt_millis: &f32);
    fn collect_incoming_messages(&mut self) -> Vec<P>;
    fn notify_message_delivered(&mut self, message_id: &MessageId);
    fn has_outgoing_messages(&self) -> bool;
    fn write_messages(
        &mut self,
        converter: &dyn NetEntityHandleConverter,
        writer: &mut BitWriter,
    ) -> Option<Vec<MessageId>>;
    fn read_messages(
        &mut self,
        reader: &mut BitReader,
        manifest: &Manifest<P>,
        converter: &dyn NetEntityHandleConverter,
    );
}
