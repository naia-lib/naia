use crate::{
    bit_reader::BitReader,
    bit_writer::BitWrite,
    error::SerdeErr,
    serde::{
        wire_schema_count, wire_schema_field, ConstBitLength, Serde, WireSchema, WireSchemaContext,
        SCHEMA_TAG_ARRAY,
    },
};

impl<T: Serde> Serde for &[T] {
    fn ser(&self, writer: &mut dyn BitWrite) {
        for item in *self {
            item.ser(writer);
        }
    }

    fn de(_: &mut BitReader) -> Result<Self, SerdeErr> {
        Err(SerdeErr {})
    }

    fn bit_length(&self) -> u32 {
        let mut output = 0;
        for item in *self {
            output += item.bit_length();
        }
        output
    }
}

impl<T: Serde, const N: usize> Serde for [T; N] {
    fn ser(&self, writer: &mut dyn BitWrite) {
        for item in self {
            item.ser(writer);
        }
    }

    fn de(reader: &mut BitReader) -> Result<Self, SerdeErr> {
        unsafe {
            let mut to = std::mem::MaybeUninit::<[T; N]>::uninit();
            let top: *mut T = &mut to as *mut std::mem::MaybeUninit<[T; N]> as *mut T;
            for c in 0..N {
                top.add(c).write(Serde::de(reader)?);
            }
            Ok(to.assume_init())
        }
    }

    fn bit_length(&self) -> u32 {
        let mut output = 0;
        for item in self {
            output += item.bit_length();
        }
        output
    }
}

impl<T: ConstBitLength, const N: usize> ConstBitLength for [T; N] {
    fn const_bit_length() -> u32 {
        T::const_bit_length() * (N as u32)
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

        let in_1: [i32; 4] = [5, 11, 52, 8];
        let in_2: [bool; 3] = [true, false, true];

        in_1.ser(&mut writer);
        in_2.ser(&mut writer);

        let buffer = writer.to_bytes();

        //Read
        let mut reader = BitReader::new(&buffer);

        let out_1: [i32; 4] = Serde::de(&mut reader).unwrap();
        let out_2: [bool; 3] = Serde::de(&mut reader).unwrap();

        assert_eq!(in_1, out_1);
        assert_eq!(in_2, out_2);
    }
}

// Schema descriptors //

// Fixed arrays carry no length on the wire, so the length is a descriptor
// fact: same element type with a different N must differ.
impl<T: WireSchema, const N: usize> WireSchema for [T; N] {
    fn wire_schema(ctx: &mut WireSchemaContext, out: &mut Vec<u8>) {
        out.push(SCHEMA_TAG_ARRAY);
        wire_schema_count(out, N as u32);
        wire_schema_field::<T>(ctx, out);
    }
}

#[cfg(test)]
mod schema_tests {
    use crate::serde::WIRE_SCHEMA_DOMAIN;
    use crate::serde::{WireSchema, SCHEMA_TAG_ARRAY, SCHEMA_TAG_BOOL};

    /// Array length is a fact; the element descriptor nests verbatim.
    #[test]
    fn array_length_and_element_type_both_matter() {
        assert_eq!(
            &<[bool; 3]>::wire_schema_bytes()[WIRE_SCHEMA_DOMAIN.len()..],
            &[SCHEMA_TAG_ARRAY, 3, 0, 0, 0, SCHEMA_TAG_BOOL],
        );
        assert_ne!(
            <[bool; 3]>::wire_schema_bytes(),
            <[bool; 4]>::wire_schema_bytes(),
            "length must differ",
        );
        assert_ne!(
            <[bool; 3]>::wire_schema_bytes(),
            <[u8; 3]>::wire_schema_bytes(),
            "element type must differ",
        );
    }
}
