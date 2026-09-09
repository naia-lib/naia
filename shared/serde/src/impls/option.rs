use crate::{
    bit_reader::BitReader,
    bit_writer::BitWrite,
    error::SerdeErr,
    serde::{
        wire_schema_field, ConstBitLength, Serde, WireSchema, WireSchemaContext, SCHEMA_TAG_OPTION,
    },
};

impl<T: Serde> Serde for Option<T> {
    fn ser(&self, writer: &mut dyn BitWrite) {
        if let Some(value) = self {
            writer.write_bit(true);
            value.ser(writer);
        } else {
            writer.write_bit(false);
        }
    }

    fn de(reader: &mut BitReader) -> Result<Option<T>, SerdeErr> {
        if reader.read_bit()? {
            Ok(Some(T::de(reader)?))
        } else {
            Ok(None)
        }
    }

    fn bit_length(&self) -> u32 {
        let mut output = 1;
        if let Some(value) = self {
            output += value.bit_length();
        }
        output
    }
}

impl<T: ConstBitLength> ConstBitLength for Option<T> {
    fn const_bit_length() -> u32 {
        1 + T::const_bit_length()
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

        let in_1 = Some(123);
        let in_2: Option<f32> = None;

        in_1.ser(&mut writer);
        in_2.ser(&mut writer);

        let buffer = writer.to_bytes();

        //Read
        let mut reader = BitReader::new(&buffer);

        let out_1 = Option::<u8>::de(&mut reader).unwrap();
        let out_2 = Option::<f32>::de(&mut reader).unwrap();

        assert_eq!(in_1, out_1);
        assert_eq!(in_2, out_2);
    }
}

// Schema descriptors //

// The present bit is fixed grammar; the descriptor is the tag plus the
// payload descriptor, so swapping the payload type changes it.
impl<T: WireSchema> WireSchema for Option<T> {
    fn wire_schema(ctx: &mut WireSchemaContext, out: &mut Vec<u8>) {
        out.push(SCHEMA_TAG_OPTION);
        wire_schema_field::<T>(ctx, out);
    }
}

#[cfg(test)]
mod schema_tests {
    use crate::serde::WIRE_SCHEMA_DOMAIN;
    use crate::serde::{WireSchema, SCHEMA_TAG_BOOL, SCHEMA_TAG_INTEGER, SCHEMA_TAG_OPTION};

    /// The present bit is fixed grammar, so the descriptor is the tag plus
    /// the payload descriptor verbatim: payload swaps and nesting depth
    /// stay distinguishable.
    #[test]
    fn option_wraps_its_payload_descriptor() {
        assert_eq!(
            &Option::<u8>::wire_schema_bytes()[WIRE_SCHEMA_DOMAIN.len()..],
            &[SCHEMA_TAG_OPTION, SCHEMA_TAG_INTEGER, 0, 0, 8],
            "option must nest its payload verbatim",
        );
        assert_eq!(
            &Option::<bool>::wire_schema_bytes()[WIRE_SCHEMA_DOMAIN.len()..],
            &[SCHEMA_TAG_OPTION, SCHEMA_TAG_BOOL],
        );
        assert_ne!(
            Option::<u8>::wire_schema_bytes(),
            Option::<u16>::wire_schema_bytes(),
            "payload swap must differ",
        );
        assert_ne!(
            Option::<u8>::wire_schema_bytes(),
            Option::<Option<u8>>::wire_schema_bytes(),
            "nesting depth must differ",
        );
    }
}
