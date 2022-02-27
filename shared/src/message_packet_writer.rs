use naia_serde::{BitWrite, Serde};

use super::{
    manager_type::ManagerType,
    protocolize::{ProtocolKindType, Protocolize},
};

/// Handles writing of Message data into an outgoing packet
pub struct MessagePacketWriter {
    queue_count: u8,
}

impl MessagePacketWriter {
    /// Construct a new instance of `MessagePacketWriter`, the given `buffer`
    /// will be used to read information from.
    pub fn new() -> MessagePacketWriter {
        MessagePacketWriter { queue_count: 0 }
    }

    /// Writes an Message into the Writer's internal buffer, which will
    /// eventually be put into the outgoing packet
    pub fn write_message<P: Protocolize, S: BitWrite>(&mut self, writer: &mut S, message: &P) {
        // write message kind
        message.dyn_ref().kind().to_u16().ser(writer);

        // write payload
        message.write(writer);

        self.queue_count += 1;
    }

    /// Write bytes into an outgoing packet
    pub fn write_header<S: BitWrite>(&mut self, writer: &mut S) {
        //Write manager "header" (manager type & message count)

        // write manager type
        ManagerType::Message.ser(writer);

        // write number of messages
        self.queue_count.ser(writer);

        self.queue_count = 0;
    }
}
