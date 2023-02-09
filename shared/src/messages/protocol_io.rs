use crate::messages::message_kinds::MessageKinds;
use crate::{
    messages::message_channel::{ChannelReader, ChannelWriter},
    Message, NetEntityHandleConverter,
};
use naia_serde::{BitReader, BitWrite, SerdeErr};

pub struct ProtocolIo<'c> {
    converter: &'c dyn NetEntityHandleConverter,
}

impl<'c> ProtocolIo<'c> {
    pub fn new(converter: &'c dyn NetEntityHandleConverter) -> Self {
        Self { converter }
    }
}

impl<'c> ChannelWriter<Box<dyn Message>> for ProtocolIo<'c> {
    fn write(
        &self,
        message_kinds: &MessageKinds,
        writer: &mut dyn BitWrite,
        data: &Box<dyn Message>,
    ) {
        data.write(message_kinds, writer, self.converter);
    }
}

impl<'c> ChannelReader<Box<dyn Message>> for ProtocolIo<'c> {
    fn read(
        &self,
        message_kinds: &MessageKinds,
        reader: &mut BitReader,
    ) -> Result<Box<dyn Message>, SerdeErr> {
        message_kinds.read(reader, self.converter)
    }
}
