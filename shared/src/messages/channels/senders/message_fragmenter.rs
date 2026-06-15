use naia_serde::{BitWrite, BitWriter};

use crate::{
    constants::FRAGMENTATION_LIMIT_BITS,
    messages::fragment::{FragmentId, FragmentIndex, FragmentedMessage},
    LocalEntityAndGlobalEntityConverterMut, MessageContainer, MessageKinds,
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
        converter: &mut dyn LocalEntityAndGlobalEntityConverterMut,
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

    #[allow(clippy::wrong_self_convention)]
    fn to_messages(mut self) -> Vec<MessageContainer> {
        self.flush_current();

        let mut output = Vec::with_capacity(self.fragments.len());

        for mut fragment in self.fragments {
            fragment.set_total(self.current_fragment_index);
            output.push(MessageContainer::new(Box::new(fragment)));
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
        let mut temp = byte;
        for _ in 0..8 {
            self.write_bit(temp & 1 != 0);
            temp >>= 1;
        }
    }

    fn count_bits(&mut self, _bits: u32) {
        panic!("This method should only be used by BitCounter");
    }

    fn is_counter(&self) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    //! Fragment-boundary invariant: the per-fragment writer holds exactly
    //! `FRAGMENTATION_LIMIT_BITS`, so a body that fits in that many bits is one
    //! fragment and one bit more rolls to a second. This is the off-by-one that
    //! would silently corrupt/lose payload at the boundary, and it's the reason
    //! the send-side gates (`message_manager::send_message` and
    //! `ReliableMessageSender::send_or_fragment`) correctly use `>` not `>=`: a
    //! body of exactly the limit fits whole, so fragmenting it would only add
    //! needless fragment-header overhead. Driving `FragmentWriter` through its
    //! `BitWrite` impl pins this with no Message/MessageKinds/converter setup.

    use super::FragmentWriter;
    use crate::{constants::FRAGMENTATION_LIMIT_BITS, messages::fragment::FragmentId};
    use naia_serde::BitWrite;

    fn fragments_for_bits(bit_count: u32) -> usize {
        let mut writer = FragmentWriter::new(FragmentId::zero());
        for _ in 0..bit_count {
            writer.write_bit(true);
        }
        writer.to_messages().len()
    }

    #[test]
    fn a_body_filling_exactly_one_fragment_is_not_split() {
        assert_eq!(
            fragments_for_bits(FRAGMENTATION_LIMIT_BITS),
            1,
            "a body of exactly FRAGMENTATION_LIMIT_BITS must occupy a single fragment"
        );
    }

    #[test]
    fn one_bit_past_the_limit_rolls_into_a_second_fragment() {
        assert_eq!(
            fragments_for_bits(FRAGMENTATION_LIMIT_BITS + 1),
            2,
            "one bit beyond the limit must spill into a second fragment"
        );
    }

    #[test]
    fn fragment_count_scales_with_whole_multiples_of_the_limit() {
        // Three full fragments exactly; the fourth bit would start a 4th.
        assert_eq!(fragments_for_bits(FRAGMENTATION_LIMIT_BITS * 3), 3);
        assert_eq!(fragments_for_bits(FRAGMENTATION_LIMIT_BITS * 3 + 1), 4);
    }
}
