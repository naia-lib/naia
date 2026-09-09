use crate::{
    bit_reader::BitReader,
    bit_writer::BitWrite,
    error::SerdeErr,
    serde::{
        wire_schema_field, ConstBitLength, Serde, WireSchema, WireSchemaContext, SCHEMA_TAG_BYTES,
    },
    UnsignedVariableInteger,
};

impl<T: Serde> Serde for Box<T> {
    fn ser(&self, writer: &mut dyn BitWrite) {
        (**self).ser(writer)
    }

    fn de(reader: &mut BitReader) -> Result<Box<T>, SerdeErr> {
        Ok(Box::new(Serde::de(reader)?))
    }

    fn bit_length(&self) -> u32 {
        (**self).bit_length()
    }
}

impl<T: ConstBitLength> ConstBitLength for Box<T> {
    fn const_bit_length() -> u32 {
        T::const_bit_length()
    }
}

impl Serde for Box<[u8]> {
    fn ser(&self, writer: &mut dyn BitWrite) {
        let length = UnsignedVariableInteger::<9>::new(self.len() as u64);
        length.ser(writer);
        let bytes: &[u8] = self;
        for byte in bytes {
            writer.write_byte(*byte);
        }
    }

    fn de(reader: &mut BitReader) -> Result<Box<[u8]>, SerdeErr> {
        let length_int = UnsignedVariableInteger::<9>::de(reader)?;
        let length_usize = length_int.get() as usize;
        // `length_usize` comes off the wire, so a peer picks it freely; reserving it
        // directly lets a handful of bytes demand gigabytes. Each element here is a
        // whole byte, so the bits left in the reader bound how many can actually
        // follow. The loop below is already self-limiting -- `read_byte` fails once
        // the reader runs dry -- so only the pre-allocation needed a bound.
        let mut bytes: Vec<u8> = Vec::with_capacity(length_usize.min(reader.bits_remaining() / 8));
        for _ in 0..length_usize {
            bytes.push(reader.read_byte()?);
        }

        Ok(bytes.into_boxed_slice())
    }

    fn bit_length(&self) -> u32 {
        let mut output = 0;
        let length = UnsignedVariableInteger::<9>::new(self.len() as u64);
        output += length.bit_length();
        output += (self.len() as u32) * 8;
        output
    }
}

// Tests

#[cfg(test)]
mod tests {
    use crate::{bit_reader::BitReader, bit_writer::BitWriter, serde::Serde};

    #[test]
    fn read_write() {
        // Write
        let mut writer = BitWriter::new();

        let in_1 = Box::new(123);
        let in_2 = Box::new(true);

        in_1.ser(&mut writer);
        in_2.ser(&mut writer);

        let buffer = writer.to_bytes();

        //Read
        let mut reader = BitReader::new(&buffer);

        let out_1 = Box::<u8>::de(&mut reader).unwrap();
        let out_2 = Box::<bool>::de(&mut reader).unwrap();

        assert_eq!(in_1, out_1);
        assert_eq!(in_2, out_2);
    }
}

// Schema descriptors //

// `Box<T>` is transparent: it emits `T`'s descriptor unchanged, routing the
// child through the canonical field path. That routing is what makes
// recursion terminate: `Box<Self>` enters `Box<Self>` on the traversal
// stack, so the inner `Self` folds to the already-active root (BACKREF 0)
// instead of re-emitting through a direct call that never consults the
// stack. `Box<[u8]>` is length-prefixed bytes on the wire, like `String`.
impl<T: WireSchema> WireSchema for Box<T> {
    fn wire_schema(ctx: &mut WireSchemaContext, out: &mut Vec<u8>) {
        wire_schema_field::<T>(ctx, out);
    }
}

impl WireSchema for Box<[u8]> {
    fn wire_schema(ctx: &mut WireSchemaContext, out: &mut Vec<u8>) {
        out.push(SCHEMA_TAG_BYTES);
        wire_schema_field::<UnsignedVariableInteger<9>>(ctx, out);
    }
}

#[cfg(test)]
mod schema_tests {
    use crate::serde::WIRE_SCHEMA_DOMAIN;
    use crate::serde::{
        wire_schema_count, wire_schema_field, wire_schema_label, WireSchema, WireSchemaContext,
        SCHEMA_TAG_BACKREF, SCHEMA_TAG_BYTES, SCHEMA_TAG_INTEGER, SCHEMA_TAG_STRUCT,
    };

    /// A self-containing type whose field routes through the canonical
    /// field path, exactly as every generated `WireSchema` impl does.
    /// Never constructed — only described — so the field is appeal-proofed
    /// against dead-code lint.
    #[allow(dead_code)]
    struct Chain {
        next: Box<Chain>,
    }

    impl WireSchema for Chain {
        fn wire_schema(ctx: &mut WireSchemaContext, out: &mut Vec<u8>) {
            out.push(SCHEMA_TAG_STRUCT);
            wire_schema_count(out, 1);
            wire_schema_label(out, "next");
            wire_schema_field::<Box<Chain>>(ctx, out);
        }
    }

    /// Recursion through `Box<Self>` folds to the active root (`BACKREF 0`)
    /// with no spurious inlined level.
    ///
    /// This falsifies direct-child dispatch (`T::wire_schema` inside the
    /// `Box` impl): bypassing the field path never pushes `Box<Self>`, so
    /// the inner `Self` re-emits a whole nested STRUCT level and folds to
    /// `BACKREF 1` instead. The exact-bytes assertion below would fail on
    /// that shape — no abort, just a red test.
    #[test]
    fn recursive_box_folds_to_backref_zero() {
        let mut expected = Vec::new();
        expected.extend_from_slice(WIRE_SCHEMA_DOMAIN);
        expected.push(SCHEMA_TAG_STRUCT);
        wire_schema_count(&mut expected, 1);
        wire_schema_label(&mut expected, "next");
        expected.push(SCHEMA_TAG_BACKREF);
        expected.extend_from_slice(&0u32.to_le_bytes());

        assert_eq!(Chain::wire_schema_bytes(), expected);
    }

    /// Transparency means byte-identity with the inner type; byte boxes
    /// carry their length codec.
    #[test]
    fn box_is_transparent_and_byte_boxes_carry_their_codec() {
        assert_eq!(
            Box::<u8>::wire_schema_bytes(),
            u8::wire_schema_bytes(),
            "Box<T> must describe exactly as T",
        );
        assert_eq!(
            Box::<Box<u8>>::wire_schema_bytes(),
            u8::wire_schema_bytes(),
            "nesting transparent boxes changes nothing",
        );
        assert_eq!(
            &Box::<[u8]>::wire_schema_bytes()[WIRE_SCHEMA_DOMAIN.len()..],
            &[SCHEMA_TAG_BYTES, SCHEMA_TAG_INTEGER, 0, 1, 9],
        );
    }
}
