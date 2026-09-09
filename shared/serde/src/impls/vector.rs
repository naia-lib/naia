use std::collections::VecDeque;

use crate::{
    bit_reader::BitReader,
    bit_writer::BitWrite,
    error::SerdeErr,
    serde::{
        wire_schema_field, Serde, WireSchema, WireSchemaContext, SCHEMA_ORDERED, SCHEMA_TAG_VECTOR,
    },
    UnsignedVariableInteger,
};

impl<T: Serde> Serde for Vec<T> {
    fn ser(&self, writer: &mut dyn BitWrite) {
        let length = UnsignedVariableInteger::<5>::new(self.len() as u64);
        length.ser(writer);
        for item in self {
            item.ser(writer);
        }
    }

    fn de(reader: &mut BitReader) -> Result<Self, SerdeErr> {
        let length_int = UnsignedVariableInteger::<5>::de(reader)?;
        let length_usize = length_int.get() as usize;
        // `length_usize` comes off the wire, so a peer picks it freely. Reserving it
        // directly lets a handful of bytes demand gigabytes before a single element
        // is decoded. Every element costs at least one bit, so what remains in the
        // reader is a hard ceiling on how many can actually follow; reserve the
        // smaller of the two and let the loop grow the collection if the peer was
        // honest. The loop is already self-limiting -- `T::de` fails once the
        // reader runs dry.
        let mut output: Vec<T> = Vec::with_capacity(length_usize.min(reader.bits_remaining()));
        for _ in 0..length_usize {
            output.push(T::de(reader)?)
        }
        Ok(output)
    }

    fn bit_length(&self) -> u32 {
        let mut output = 0;
        let length = UnsignedVariableInteger::<5>::new(self.len() as u64);
        output += length.bit_length();
        for item in self {
            output += item.bit_length();
        }
        output
    }
}

impl<T: Serde> Serde for VecDeque<T> {
    fn ser(&self, writer: &mut dyn BitWrite) {
        let length = UnsignedVariableInteger::<5>::new(self.len() as u64);
        length.ser(writer);
        for item in self {
            item.ser(writer);
        }
    }

    fn de(reader: &mut BitReader) -> Result<Self, SerdeErr> {
        let length_int = UnsignedVariableInteger::<5>::de(reader)?;
        let length_usize = length_int.get() as usize;
        // `length_usize` comes off the wire, so a peer picks it freely. Reserving it
        // directly lets a handful of bytes demand gigabytes before a single element
        // is decoded. Every element costs at least one bit, so what remains in the
        // reader is a hard ceiling on how many can actually follow; reserve the
        // smaller of the two and let the loop grow the collection if the peer was
        // honest. The loop is already self-limiting -- `T::de` fails once the
        // reader runs dry.
        let mut output: VecDeque<T> =
            VecDeque::with_capacity(length_usize.min(reader.bits_remaining()));
        for _ in 0..length_usize {
            output.push_back(T::de(reader)?)
        }
        Ok(output)
    }

    fn bit_length(&self) -> u32 {
        let mut output = 0;
        let length = UnsignedVariableInteger::<5>::new(self.len() as u64);
        output += length.bit_length();
        for item in self {
            output += item.bit_length();
        }
        output
    }
}

// Tests

#[cfg(test)]
mod tests {
    use crate::{bit_reader::BitReader, bit_writer::BitWriter, serde::Serde};
    use std::collections::VecDeque;

    #[test]
    fn read_write_vec() {
        // Write
        let mut writer = BitWriter::new();

        let in_1 = vec![5, 3, 2, 7];
        let in_2 = vec![false, false, true, false, true, true, false, true];

        in_1.ser(&mut writer);
        in_2.ser(&mut writer);

        let buffer = writer.to_bytes();

        // Read
        let mut reader = BitReader::new(&buffer);

        let out_1: Vec<i32> = Serde::de(&mut reader).unwrap();
        let out_2: Vec<bool> = Serde::de(&mut reader).unwrap();

        assert_eq!(in_1, out_1);
        assert_eq!(in_2, out_2);
    }

    #[test]
    fn read_write_vec_deque() {
        // Write
        let mut writer = BitWriter::new();

        let mut in_1 = VecDeque::<i32>::new();
        in_1.push_back(5);
        in_1.push_back(2);
        in_1.push_back(-7);
        in_1.push_back(331);
        in_1.push_back(-527);
        let mut in_2 = VecDeque::<bool>::new();
        in_2.push_back(true);
        in_2.push_back(false);
        in_2.push_back(false);
        in_2.push_back(true);
        in_2.push_back(false);
        in_2.push_back(true);
        in_2.push_back(true);
        in_2.push_back(true);

        in_1.ser(&mut writer);
        in_2.ser(&mut writer);

        let buffer = writer.to_bytes();

        // Read
        let mut reader = BitReader::new(&buffer);

        let out_1: VecDeque<i32> = Serde::de(&mut reader).unwrap();
        let out_2: VecDeque<bool> = Serde::de(&mut reader).unwrap();

        assert_eq!(in_1, out_1);
        assert_eq!(in_2, out_2);
    }
}

// Schema descriptors //

// Ordered collections: the order class, the element descriptor, and the
// exact length codec (`UnsignedVariableInteger<5>`) are all facts.
impl<T: WireSchema> WireSchema for Vec<T> {
    fn wire_schema(ctx: &mut WireSchemaContext, out: &mut Vec<u8>) {
        out.push(SCHEMA_TAG_VECTOR);
        out.push(SCHEMA_ORDERED);
        wire_schema_field::<T>(ctx, out);
        wire_schema_field::<UnsignedVariableInteger<5>>(ctx, out);
    }
}

impl<T: WireSchema> WireSchema for VecDeque<T> {
    fn wire_schema(ctx: &mut WireSchemaContext, out: &mut Vec<u8>) {
        out.push(SCHEMA_TAG_VECTOR);
        out.push(SCHEMA_ORDERED);
        wire_schema_field::<T>(ctx, out);
        wire_schema_field::<UnsignedVariableInteger<5>>(ctx, out);
    }
}

#[cfg(test)]
mod schema_tests {
    use crate::serde::WIRE_SCHEMA_DOMAIN;
    use crate::serde::{WireSchema, SCHEMA_ORDERED, SCHEMA_TAG_INTEGER, SCHEMA_TAG_VECTOR};

    /// Element type and length codec are facts; Vec and VecDeque share the
    /// ordered wire grammar, so they share the descriptor shape.
    #[test]
    fn ordered_collections_carry_element_and_codec() {
        let bytes = Vec::<u8>::wire_schema_bytes();
        assert_eq!(bytes[WIRE_SCHEMA_DOMAIN.len()], SCHEMA_TAG_VECTOR);
        assert_eq!(bytes[WIRE_SCHEMA_DOMAIN.len() + 1], SCHEMA_ORDERED);
        assert_eq!(
            &bytes[WIRE_SCHEMA_DOMAIN.len() + 2..],
            &[SCHEMA_TAG_INTEGER, 0, 0, 8, SCHEMA_TAG_INTEGER, 0, 1, 5][..],
            "elem then UVI<5> codec, both verbatim",
        );
        assert_eq!(
            Vec::<u8>::wire_schema_bytes(),
            std::collections::VecDeque::<u8>::wire_schema_bytes(),
            "same grammar must describe identically",
        );
        assert_ne!(
            Vec::<u8>::wire_schema_bytes(),
            Vec::<u16>::wire_schema_bytes(),
            "element swap must differ",
        );
    }
}
