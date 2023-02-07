use crate::{
    messages::message_channel::{ChannelReader, ChannelWriter},
    Message, Messages, NetEntityHandleConverter,
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
    fn write(&self, writer: &mut dyn BitWrite, data: &Box<dyn Message>) {
        data.write(writer, self.converter);
    }
}

impl<'c> ChannelReader<Box<dyn Message>> for ProtocolIo<'c> {
    fn read(&self, reader: &mut BitReader) -> Result<Box<dyn Message>, SerdeErr> {
        Messages::read(reader, self.converter)
    }
}
