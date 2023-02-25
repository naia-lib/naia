use naia_derive::MessageInternal;
use naia_serde::{BitWrite, BitWriter};

use crate::{
    constants::FRAGMENTATION_LIMIT_BITS, MessageContainer, MessageKinds, NetEntityHandleConverter,
};

// BitFragmenter
pub struct BitFragmenter {
    fragments: Vec<MessageContainer>,
    current_writer: BitWriter,
}

impl BitFragmenter {
    pub fn fragment_message(
        message_kinds: &MessageKinds,
        converter: &dyn NetEntityHandleConverter,
        message: MessageContainer,
    ) -> Vec<MessageContainer> {
        let mut fragmenter = BitFragmenter::new();
        message.write(message_kinds, &mut fragmenter, converter);
        fragmenter.to_messages()
    }

    fn new() -> Self {
        Self {
            fragments: Vec::new(),
            current_writer: BitWriter::with_capacity(FRAGMENTATION_LIMIT_BITS),
        }
    }

    fn flush_current(&mut self) {
        let current = std::mem::replace(
            &mut self.current_writer,
            BitWriter::with_capacity(FRAGMENTATION_LIMIT_BITS),
        );
        let bytes = current.to_vec();
        let fragmented_message = FragmentedMessage::from(bytes);
        let boxed_message = Box::new(fragmented_message);
        self.fragments.push(MessageContainer::from(boxed_message));
    }

    fn to_messages(mut self) -> Vec<MessageContainer> {
        self.flush_current();
        self.fragments
    }
}

impl BitWrite for BitFragmenter {
    fn write_bit(&mut self, bit: bool) {
        if self.current_writer.bits_free() == 0 {
            self.flush_current();
        }
        self.current_writer.write_bit(bit);
    }

    fn write_byte(&mut self, byte: u8) {
        if self.current_writer.bits_free() < 8 {
            self.flush_current();
        }
        self.current_writer.write_byte(byte);
    }

    fn write_bits(&mut self, _bits: u32) {
        panic!("This method only to be used by BitCounter");
    }

    fn is_counter(&self) -> bool {
        false
    }
}

#[derive(MessageInternal)]
pub struct FragmentedMessage {
    inner: Vec<u8>,
}

impl FragmentedMessage {
    pub fn from(inner: Vec<u8>) -> Self {
        Self { inner }
    }
}
