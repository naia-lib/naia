use naia_serde::{BitWrite, BitWriter};

use crate::messages::fragment::{FragmentIndex, FragmentedMessage};
use crate::{
    constants::FRAGMENTATION_LIMIT_BITS, messages::fragment::FragmentId, MessageContainer,
    MessageKinds, NetEntityHandleConverter,
};

// MessageFragmenter
pub struct MessageFragmenter {
    current_fragment_id: FragmentId,
}

impl MessageFragmenter {
    pub fn new() -> Self {
        Self {
            current_fragment_id: FragmentId::zero(),
        }
    }

    pub fn fragment_message(
        &mut self,
        message_kinds: &MessageKinds,
        converter: &dyn NetEntityHandleConverter,
        message: MessageContainer,
    ) -> Vec<MessageContainer> {
        let mut fragmenter = FragmentWriter::new(self.current_fragment_id);
        self.current_fragment_id.increment();
        message.write(message_kinds, &mut fragmenter, converter);
        fragmenter.to_messages()
    }
}

// FragmentWriter
pub struct FragmentWriter {
    fragment_id: FragmentId,
    current_fragment_index: FragmentIndex,
    fragments: Vec<FragmentedMessage>,
    current_writer: BitWriter,
}

impl FragmentWriter {
    fn new(id: FragmentId) -> Self {
        Self {
            fragment_id: id,
            current_fragment_index: FragmentIndex::zero(),
            fragments: Vec::new(),
            current_writer: BitWriter::with_capacity(FRAGMENTATION_LIMIT_BITS),
        }
    }

    fn flush_current(&mut self) {
        let current = std::mem::replace(
            &mut self.current_writer,
            BitWriter::with_capacity(FRAGMENTATION_LIMIT_BITS),
        );
        let bytes = current.to_bytes();
        let fragmented_message =
            FragmentedMessage::new(self.fragment_id, self.current_fragment_index, bytes);
        self.current_fragment_index.increment();
        self.fragments.push(fragmented_message);
    }

    fn to_messages(mut self) -> Vec<MessageContainer> {
        self.flush_current();

        let mut output = Vec::with_capacity(self.fragments.len());

        for mut fragment in self.fragments {
            fragment.set_total(self.current_fragment_index);
            output.push(MessageContainer::from(Box::new(fragment)));
        }

        output
    }
}

impl BitWrite for FragmentWriter {
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
        panic!("This method should only be used by BitCounter");
    }

    fn is_counter(&self) -> bool {
        false
    }
}
