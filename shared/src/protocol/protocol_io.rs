use crate::{
    messages::message_channel::{ChannelReader, ChannelWriter},
    NetEntityHandleConverter, Protocolize,
};
use naia_serde::{BitReader, BitWrite, Serde};

pub struct ProtocolIo<'c> {
    converter: &'c dyn NetEntityHandleConverter,
}

impl<'c> ProtocolIo<'c> {
    pub fn new(converter: &'c dyn NetEntityHandleConverter) -> Self {
        Self { converter }
    }
}

impl<'c, P: Protocolize> ChannelWriter<P> for ProtocolIo<'c> {
    fn write(&self, bit_writer: &mut dyn BitWrite, data: &P) {
        // write message kind
        data.dyn_ref().kind().ser(bit_writer);

        // write payload
        data.write(bit_writer, self.converter);
    }
}

impl<'c, P: Protocolize> ChannelReader<P> for ProtocolIo<'c> {
    fn read(&self, bit_reader: &mut BitReader) -> P {
        P::build(bit_reader, self.converter)
    }
}
