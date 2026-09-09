use crate::{
    bit_reader::BitReader,
    bit_writer::BitWrite,
    error::SerdeErr,
    serde::{wire_schema_field, Serde, WireSchema, WireSchemaContext, SCHEMA_TAG_STRING},
    UnsignedVariableInteger,
};

impl Serde for String {
    fn ser(&self, writer: &mut dyn BitWrite) {
        let length = UnsignedVariableInteger::<9>::new(self.len() as u64);
        length.ser(writer);
        let bytes = self.as_bytes();
        for byte in bytes {
            writer.write_byte(*byte);
        }
    }

    fn de(reader: &mut BitReader) -> Result<Self, SerdeErr> {
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

        let result = String::from_utf8_lossy(&bytes).into_owned();
        Ok(result)
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

        let in_1 = "Hello world!".to_string();
        let in_2 = "This is a string.".to_string();

        in_1.ser(&mut writer);
        in_2.ser(&mut writer);

        let buffer = writer.to_bytes();

        // Read
        let mut reader = BitReader::new(&buffer);

        let out_1: String = Serde::de(&mut reader).unwrap();
        let out_2: String = Serde::de(&mut reader).unwrap();

        assert_eq!(in_1, out_1);
        assert_eq!(in_2, out_2);
    }
}

// Schema descriptors //

// A string is a `UnsignedVariableInteger<9>` length followed by raw bytes,
// so the descriptor is the codec's own descriptor nested under the tag.
impl WireSchema for String {
    fn wire_schema(ctx: &mut WireSchemaContext, out: &mut Vec<u8>) {
        out.push(SCHEMA_TAG_STRING);
        wire_schema_field::<UnsignedVariableInteger<9>>(ctx, out);
    }
}

#[cfg(test)]
mod schema_tests {
    use crate::serde::WIRE_SCHEMA_DOMAIN;
    use crate::{
        serde::{WireSchema, SCHEMA_TAG_INTEGER, SCHEMA_TAG_STRING},
        UnsignedVariableInteger,
    };

    /// The string descriptor carries its exact length codec: re-coding the
    /// length with a different width must change it.
    #[test]
    fn string_descriptor_carries_its_length_codec() {
        let bytes = String::wire_schema_bytes();
        assert_eq!(bytes[WIRE_SCHEMA_DOMAIN.len()], SCHEMA_TAG_STRING);
        assert_eq!(
            &bytes[WIRE_SCHEMA_DOMAIN.len() + 1..],
            &UnsignedVariableInteger::<9>::wire_schema_bytes()[WIRE_SCHEMA_DOMAIN.len()..],
            "the codec descriptor must be nested verbatim",
        );
        assert_eq!(
            &bytes[WIRE_SCHEMA_DOMAIN.len() + 1..],
            &[SCHEMA_TAG_INTEGER, 0, 1, 9]
        );
    }
}
