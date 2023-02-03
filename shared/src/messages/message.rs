use crate::messages::named::Named;
use crate::{MessageId, NetEntityHandleConverter};
use naia_serde::{BitReader, BitWrite, Serde, SerdeErr};

/// A map to hold all component types
pub struct Messages {}

impl Messages {}

impl Messages {
    pub fn kind_of<R: Message>() -> MessageId {
        todo!()
    }

    pub fn read(
        bit_reader: &mut BitReader,
        converter: &dyn NetEntityHandleConverter,
    ) -> Result<Box<dyn Message>, SerdeErr> {
        todo!()
    }

    pub fn write<M: Message>(
        bit_writer: &mut dyn BitWrite,
        converter: &dyn NetEntityHandleConverter,
        message: &M,
    ) {
        todo!()
    }
}

pub trait Message: Send + Sync + MessageClone + Named {}

pub trait MessageClone {
    fn clone_box(&self) -> Box<dyn Message>;
}

impl<T: 'static + Clone + Message> MessageClone for T {
    fn clone_box(&self) -> Box<dyn Message> {
        Box::new(self.clone())
    }
}

impl Clone for Box<dyn Message> {
    fn clone(&self) -> Box<dyn Message> {
        MessageClone::clone_box(self.as_ref())
    }
}

impl Named for Box<dyn Message> {
    fn name(&self) -> String {
        Named::name(self.as_ref())
    }
}
