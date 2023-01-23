use naia_serde::{BitReader, BitWrite, BitWriter, SerdeErr};
use naia_socket_shared::Instant;

use crate::types::MessageId;

pub trait ChannelSender<P>: Send + Sync {
    fn send_message(&mut self, message: P);
    fn collect_messages(&mut self, now: &Instant, rtt_millis: &f32);
    fn has_messages(&self) -> bool;
    fn write_messages(
        &mut self,
        channel_writer: &dyn ChannelWriter<P>,
        bit_writer: &mut BitWriter,
        has_written: &mut bool,
    ) -> Option<Vec<MessageId>>;
    fn notify_message_delivered(&mut self, message_id: &MessageId);
}

pub trait ChannelReceiver<P>: Send + Sync {
    fn read_messages(
        &mut self,
        channel_reader: &dyn ChannelReader<P>,
        reader: &mut BitReader,
    ) -> Result<(), SerdeErr>;
    fn receive_messages(&mut self) -> Vec<P>;
}

pub trait ChannelWriter<T> {
    fn write(&self, writer: &mut dyn BitWrite, data: &T);
}

pub trait ChannelReader<T> {
    fn read(&self, reader: &mut BitReader) -> Result<T, SerdeErr>;
}
