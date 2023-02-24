use naia_serde::{BitReader, BitWrite, SerdeErr};

use crate::{messages::message_kinds::MessageKinds, Message, NetEntityHandleConverter};

pub struct ProtocolIo<'c> {
    converter: &'c dyn NetEntityHandleConverter,
}

impl<'c> ProtocolIo<'c> {
    pub fn new(converter: &'c dyn NetEntityHandleConverter) -> Self {
        Self { converter }
    }

    pub fn write(
        &self,
        message_kinds: &MessageKinds,
        writer: &mut dyn BitWrite,
        data: &Box<dyn Message>,
    ) {
        data.write(message_kinds, writer, self.converter);
    }

    pub fn read(
        &self,
        message_kinds: &MessageKinds,
        reader: &mut BitReader,
    ) -> Result<Box<dyn Message>, SerdeErr> {
        message_kinds.read(reader, self.converter)
    }
}
