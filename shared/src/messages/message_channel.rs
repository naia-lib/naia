use naia_serde::{BitReader, BitWriter, SerdeErr};
use naia_socket_shared::Instant;

use crate::{messages::message_kinds::MessageKinds, types::MessageIndex, Message, ProtocolIo};

pub trait ChannelSender<P>: Send + Sync {
    /// Queues a Message to be transmitted to the remote host into an internal buffer
    fn send_message(&mut self, message: P);
    /// For reliable channels, will collect any Messages that need to be resent
    fn collect_messages(&mut self, now: &Instant, rtt_millis: &f32);
    /// Returns true if there are queued Messages ready to be written
    fn has_messages(&self) -> bool;
    /// Called when it receives acknowledgement that a Message has been received
    fn notify_message_delivered(&mut self, message_index: &MessageIndex);
}

pub trait MessageChannelSender: ChannelSender<Box<dyn Message>> {
    /// Gets Messages from the internal buffer and writes it to the channel_writer
    fn write_messages(
        &mut self,
        message_kinds: &MessageKinds,
        channel_writer: &ProtocolIo,
        bit_writer: &mut BitWriter,
        has_written: &mut bool,
    ) -> Option<Vec<MessageIndex>>;
}

pub trait ChannelReceiver<P>: Send + Sync {
    /// Read messages from an internal buffer and return their content
    fn receive_messages(&mut self) -> Vec<P>;
}

pub trait MessageChannelReceiver: ChannelReceiver<Box<dyn Message>> {
    /// Read messages from raw bits, parse them and store then into an internal buffer
    fn read_messages(
        &mut self,
        message_kinds: &MessageKinds,
        channel_reader: &ProtocolIo,
        reader: &mut BitReader,
    ) -> Result<(), SerdeErr>;
}
