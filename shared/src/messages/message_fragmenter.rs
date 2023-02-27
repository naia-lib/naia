use naia_derive::MessageFragment;
use naia_serde::{
    BitReader, BitWrite, BitWriter, ConstBitLength, Serde, SerdeErr, UnsignedInteger,
};

use crate::{
    constants::FRAGMENTATION_LIMIT_BITS, MessageContainer, MessageKinds, NetEntityHandleConverter,
};

const FRAGMENT_ID_BITS: u8 = 10;
const FRAGMENT_ID_LIMIT: u16 = 2 ^ (FRAGMENT_ID_BITS as u16);
const FRAGMENT_INDEX_BITS: u8 = 20;
const FRAGMENT_INDEX_LIMIT: u32 = 2 ^ (FRAGMENT_INDEX_BITS as u32);

// FragmentId
#[derive(Copy, Clone, PartialEq, Eq, Hash)]
pub struct FragmentId {
    inner: u16,
}

impl FragmentId {
    fn zero() -> Self {
        Self { inner: 0 }
    }

    fn increment(&mut self) {
        self.inner += 1;
        if self.inner >= FRAGMENT_ID_LIMIT {
            self.inner = 0;
        }
    }
}

impl Serde for FragmentId {
    fn ser(&self, writer: &mut dyn BitWrite) {
        let integer = UnsignedInteger::<FRAGMENT_ID_BITS>::new(self.inner as u64);
        integer.ser(writer);
    }

    fn de(reader: &mut BitReader) -> Result<Self, SerdeErr> {
        let integer = UnsignedInteger::<FRAGMENT_ID_BITS>::de(reader)?;
        let inner = integer.get() as u16;
        Ok(Self { inner })
    }

    fn bit_length(&self) -> u32 {
        <Self as ConstBitLength>::const_bit_length()
    }
}

impl ConstBitLength for FragmentId {
    fn const_bit_length() -> u32 {
        FRAGMENT_ID_BITS as u32
    }
}

// FragmentIndex
#[derive(Copy, Clone, PartialEq)]
pub struct FragmentIndex {
    inner: u32,
}

impl FragmentIndex {
    fn zero() -> Self {
        Self { inner: 0 }
    }

    fn increment(&mut self) {
        self.inner += 1;
        if self.inner >= FRAGMENT_INDEX_LIMIT {
            panic!("Attempting to fragment large message, but hit fragment limit of {FRAGMENT_INDEX_LIMIT}. This means you're trying to transmit about 500 megabytes, which is a bad idea.")
        }
    }

    pub fn as_usize(&self) -> usize {
        self.inner as usize
    }
}

impl Serde for FragmentIndex {
    fn ser(&self, writer: &mut dyn BitWrite) {
        let integer = UnsignedInteger::<FRAGMENT_INDEX_BITS>::new(self.inner as u64);
        integer.ser(writer);
    }

    fn de(reader: &mut BitReader) -> Result<Self, SerdeErr> {
        let integer = UnsignedInteger::<FRAGMENT_INDEX_BITS>::de(reader)?;
        let inner = integer.get() as u32;
        Ok(Self { inner })
    }

    fn bit_length(&self) -> u32 {
        <Self as ConstBitLength>::const_bit_length()
    }
}

impl ConstBitLength for FragmentIndex {
    fn const_bit_length() -> u32 {
        FRAGMENT_INDEX_BITS as u32
    }
}

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

#[derive(MessageFragment)]
pub struct FragmentedMessage {
    id: FragmentId,
    index: FragmentIndex,
    total: FragmentIndex,
    bytes: Box<[u8]>,
}

impl FragmentedMessage {
    pub fn new(id: FragmentId, index: FragmentIndex, bytes: Box<[u8]>) -> Self {
        Self {
            id,
            index,
            bytes,
            total: FragmentIndex::zero(),
        }
    }

    pub(crate) fn set_total(&mut self, total: FragmentIndex) {
        self.total = total;
    }

    pub(crate) fn id(&self) -> FragmentId {
        self.id
    }

    pub(crate) fn index(&self) -> FragmentIndex {
        self.index
    }

    pub(crate) fn total(&self) -> FragmentIndex {
        self.total
    }

    pub(crate) fn to_payload(self) -> Box<[u8]> {
        self.bytes
    }
}
