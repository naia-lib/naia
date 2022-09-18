use crate::{
    messages::message_channel::{ChannelReader, ChannelWriter},
    NetEntityHandleConverter, Protocolize,
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

impl<'c, P: Protocolize> ChannelWriter<P> for ProtocolIo<'c> {
    fn write(&self, writer: &mut dyn BitWrite, data: &P) {
        data.write(writer, self.converter);
    }
}

impl<'c, P: Protocolize> ChannelReader<P> for ProtocolIo<'c> {
    fn read(&self, reader: &mut BitReader) -> Result<P, SerdeErr> {
        P::read(reader, self.converter)
    }
}
