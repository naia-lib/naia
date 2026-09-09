use crate::{
    bit_reader::BitReader,
    bit_writer::BitWrite,
    error::SerdeErr,
    serde::{
        ConstBitLength, Serde, WireSchema, WireSchemaContext, SCHEMA_NATIVE_ENDIAN,
        SCHEMA_TAG_BOOL, SCHEMA_TAG_CHAR, SCHEMA_TAG_INTEGER, SCHEMA_TAG_NATIVE, SCHEMA_TAG_UNIT,
    },
};

// Unit //

impl Serde for () {
    fn ser(&self, _: &mut dyn BitWrite) {}

    fn de(_: &mut BitReader) -> Result<Self, SerdeErr> {
        Ok(())
    }

    fn bit_length(&self) -> u32 {
        <Self as ConstBitLength>::const_bit_length()
    }
}

impl ConstBitLength for () {
    fn const_bit_length() -> u32 {
        0
    }
}

// tests

#[cfg(test)]
mod unit_tests {
    use crate::{bit_reader::BitReader, bit_writer::BitWriter, serde::Serde};

    #[test]
    fn read_write() {
        // Write
        let mut writer = BitWriter::new();

        let in_unit = ();

        in_unit.ser(&mut writer);

        let buffer = writer.to_bytes();

        //Read
        let mut reader = BitReader::new(&buffer);

        // Type annotation enforces that `Serde::de` returns the unit
        // type — that's the meaningful assertion. Comparing two `()`
        // values is trivially true (and clippy used to flag the
        // assert_eq); cargo clippy --fix removed the comparison.
        let out_unit: () = Serde::de(&mut reader).unwrap();
        let _ = (in_unit, out_unit);
    }
}

// Boolean //

impl Serde for bool {
    fn ser(&self, writer: &mut dyn BitWrite) {
        writer.write_bit(*self);
    }

    fn de(reader: &mut BitReader) -> Result<Self, SerdeErr> {
        reader.read_bit()
    }

    fn bit_length(&self) -> u32 {
        <Self as ConstBitLength>::const_bit_length()
    }
}

impl ConstBitLength for bool {
    fn const_bit_length() -> u32 {
        1
    }
}

// tests

#[cfg(test)]
mod bool_tests {
    use crate::{bit_reader::BitReader, bit_writer::BitWriter, serde::Serde};

    #[test]
    fn read_write() {
        // Write
        let mut writer = BitWriter::new();

        let in_1 = true;
        let in_2 = false;

        in_1.ser(&mut writer);
        in_2.ser(&mut writer);

        let buffer = writer.to_bytes();

        //Read
        let mut reader = BitReader::new(&buffer);

        let out_1 = Serde::de(&mut reader).unwrap();
        let out_2 = Serde::de(&mut reader).unwrap();

        assert_eq!(in_1, out_1);
        assert_eq!(in_2, out_2);
    }
}

// Characters //

impl Serde for char {
    fn ser(&self, writer: &mut dyn BitWrite) {
        let u32char = *self as u32;
        let bytes = unsafe { std::mem::transmute::<&u32, &[u8; 4]>(&u32char) };
        for byte in bytes {
            writer.write_byte(*byte);
        }
    }

    fn de(reader: &mut BitReader) -> Result<Self, SerdeErr> {
        let mut bytes = [0_u8; 4];
        for byte in &mut bytes {
            *byte = reader.read_byte()?;
        }
        let mut container = [0_u32];
        unsafe {
            std::ptr::copy_nonoverlapping(
                bytes.as_ptr().offset(0_isize) as *const u32,
                container.as_mut_ptr(),
                1,
            )
        }

        if let Some(inner_char) = char::from_u32(container[0]) {
            Ok(inner_char)
        } else {
            Err(SerdeErr {})
        }
    }

    fn bit_length(&self) -> u32 {
        <Self as ConstBitLength>::const_bit_length()
    }
}

impl ConstBitLength for char {
    fn const_bit_length() -> u32 {
        <[u8; 4] as ConstBitLength>::const_bit_length()
    }
}

// tests

#[cfg(test)]
mod char_tests {
    use crate::{bit_reader::BitReader, bit_writer::BitWriter, serde::Serde};

    #[test]
    fn read_write() {
        // Write
        let mut writer = BitWriter::new();

        let in_1 = 'O';
        let in_2 = '!';

        in_1.ser(&mut writer);
        in_2.ser(&mut writer);

        let buffer = writer.to_bytes();

        //Read
        let mut reader = BitReader::new(&buffer);

        let out_1 = Serde::de(&mut reader).unwrap();
        let out_2 = Serde::de(&mut reader).unwrap();

        assert_eq!(in_1, out_1);
        assert_eq!(in_2, out_2);
    }
}

// Integers & Floating-point Numbers //

macro_rules! impl_serde_for {
    ($impl_type:ident, $signed:expr, $float:expr) => {
        impl Serde for $impl_type {
            fn ser(&self, writer: &mut dyn BitWrite) {
                let du8 = unsafe {
                    std::mem::transmute::<&$impl_type, &[u8; std::mem::size_of::<$impl_type>()]>(
                        &self,
                    )
                };
                for byte in du8 {
                    writer.write_byte(*byte);
                }
            }

            fn de(reader: &mut BitReader) -> Result<$impl_type, SerdeErr> {
                const BYTES_LENGTH: usize = std::mem::size_of::<$impl_type>();
                let mut byte_array = [0_u8; BYTES_LENGTH];
                for index in 0..BYTES_LENGTH {
                    byte_array[index] = reader.read_byte()?;
                }
                let mut container = [0 as $impl_type];
                unsafe {
                    std::ptr::copy_nonoverlapping(
                        byte_array.as_ptr() as *const $impl_type,
                        container.as_mut_ptr() as *mut $impl_type,
                        1,
                    )
                }
                Ok(container[0])
            }

            fn bit_length(&self) -> u32 {
                <Self as ConstBitLength>::const_bit_length()
            }
        }
        impl ConstBitLength for $impl_type {
            fn const_bit_length() -> u32 {
                const BYTES_LENGTH: u32 = std::mem::size_of::<$impl_type>() as u32;
                return BYTES_LENGTH * 8;
            }
        }
        // Schema descriptor: these primitives all serialize by transmuting to
        // native-endian bytes, so the descriptor records width, value domain,
        // and the describing host's endianness.
        impl WireSchema for $impl_type {
            fn wire_schema(_ctx: &mut WireSchemaContext, out: &mut Vec<u8>) {
                out.push(SCHEMA_TAG_NATIVE);
                out.push(std::mem::size_of::<$impl_type>() as u8);
                out.push($signed);
                out.push($float);
                out.push(SCHEMA_NATIVE_ENDIAN);
            }
        }
    };
}

// number primitives
impl_serde_for!(u16, 0, 0);
impl_serde_for!(u32, 0, 0);
impl_serde_for!(u64, 0, 0);
impl_serde_for!(i16, 1, 0);
impl_serde_for!(i32, 1, 0);
impl_serde_for!(i64, 1, 0);
impl_serde_for!(f32, 0, 1);
impl_serde_for!(f64, 0, 1);

// u8
impl Serde for u8 {
    fn ser(&self, writer: &mut dyn BitWrite) {
        writer.write_byte(*self);
    }

    fn de(reader: &mut BitReader) -> Result<u8, SerdeErr> {
        reader.read_byte()
    }

    fn bit_length(&self) -> u32 {
        <Self as ConstBitLength>::const_bit_length()
    }
}

impl ConstBitLength for u8 {
    fn const_bit_length() -> u32 {
        8
    }
}

// i8
impl Serde for i8 {
    fn ser(&self, writer: &mut dyn BitWrite) {
        let du8 = unsafe { std::mem::transmute::<&i8, &u8>(self) };
        writer.write_byte(*du8);
    }

    fn de(reader: &mut BitReader) -> Result<i8, SerdeErr> {
        let byte = [reader.read_byte()?];
        let mut container = [0_i8];
        unsafe {
            std::ptr::copy_nonoverlapping(byte.as_ptr() as *const i8, container.as_mut_ptr(), 1)
        }
        Ok(container[0])
    }

    fn bit_length(&self) -> u32 {
        <Self as ConstBitLength>::const_bit_length()
    }
}

impl ConstBitLength for i8 {
    fn const_bit_length() -> u32 {
        8
    }
}

// usize
impl Serde for usize {
    fn ser(&self, writer: &mut dyn BitWrite) {
        let u64usize = *self as u64;
        let du8 = unsafe { std::mem::transmute::<&u64, &[u8; 8]>(&u64usize) };
        for byte in du8 {
            writer.write_byte(*byte);
        }
    }

    fn de(reader: &mut BitReader) -> Result<usize, SerdeErr> {
        let mut byte_array = [0_u8; 8];
        for byte in &mut byte_array {
            *byte = reader.read_byte()?;
        }
        let mut container = [0_u64];
        unsafe {
            std::ptr::copy_nonoverlapping(
                byte_array.as_ptr().offset(0_isize) as *const u64,
                container.as_mut_ptr(),
                1,
            )
        }
        Ok(container[0] as usize)
    }

    fn bit_length(&self) -> u32 {
        <Self as ConstBitLength>::const_bit_length()
    }
}

impl ConstBitLength for usize {
    fn const_bit_length() -> u32 {
        <u64 as ConstBitLength>::const_bit_length()
    }
}

// isize
impl Serde for isize {
    fn ser(&self, writer: &mut dyn BitWrite) {
        let u64usize = *self as u64;
        let du8 = unsafe { std::mem::transmute::<&u64, &[u8; 8]>(&u64usize) };
        for byte in du8 {
            writer.write_byte(*byte);
        }
    }

    fn de(reader: &mut BitReader) -> Result<isize, SerdeErr> {
        let mut byte_array = [0_u8; 8];
        for byte in &mut byte_array {
            *byte = reader.read_byte()?;
        }
        let mut container = [0_u64];
        unsafe {
            std::ptr::copy_nonoverlapping(
                byte_array.as_ptr().offset(0_isize) as *const u64,
                container.as_mut_ptr(),
                1,
            )
        }
        Ok(container[0] as isize)
    }

    fn bit_length(&self) -> u32 {
        <Self as ConstBitLength>::const_bit_length()
    }
}

impl ConstBitLength for isize {
    fn const_bit_length() -> u32 {
        <u64 as ConstBitLength>::const_bit_length()
    }
}

// tests

macro_rules! test_serde_for {
    ($impl_type:ident, $test_name:ident) => {
        #[test]
        fn $test_name() {
            use crate::{bit_reader::BitReader, bit_writer::BitWriter, serde::Serde};

            // Write
            let mut writer = BitWriter::new();

            let in_1: $impl_type = 123 as $impl_type;

            in_1.ser(&mut writer);

            let buffer = writer.to_bytes();

            //Read
            let mut reader = BitReader::new(&buffer);

            let out_1 = Serde::de(&mut reader).unwrap();

            assert_eq!(in_1, out_1);
        }
    };
}

mod number_tests {
    test_serde_for!(u8, test_u8);
    test_serde_for!(u16, test_u16);
    test_serde_for!(u32, test_u32);
    test_serde_for!(u64, test_u64);
    test_serde_for!(usize, test_usize);
    test_serde_for!(i8, test_i8);
    test_serde_for!(i16, test_i16);
    test_serde_for!(i32, test_i32);
    test_serde_for!(i64, test_i64);
    test_serde_for!(isize, test_isize);
    test_serde_for!(f32, test_f32);
    test_serde_for!(f64, test_f64);
}

// Schema descriptors //

// `()` and `bool` are fixed single-grammars: zero bits and one bit. The tag
// alone distinguishes them; there is no variable fact to record.
impl WireSchema for () {
    fn wire_schema(_ctx: &mut WireSchemaContext, out: &mut Vec<u8>) {
        out.push(SCHEMA_TAG_UNIT);
    }
}

impl WireSchema for bool {
    fn wire_schema(_ctx: &mut WireSchemaContext, out: &mut Vec<u8>) {
        out.push(SCHEMA_TAG_BOOL);
    }
}

// `char` serializes as 4 native-endian bytes, so the descriptor carries the
// describing host's endianness like every other transmute primitive.
impl WireSchema for char {
    fn wire_schema(_ctx: &mut WireSchemaContext, out: &mut Vec<u8>) {
        out.push(SCHEMA_TAG_CHAR);
        out.push(SCHEMA_NATIVE_ENDIAN);
    }
}

// `u8`/`i8` are single bytes on the wire -- endian-neutral -- so they are
// fixed 8-bit integers, not natives.
impl WireSchema for u8 {
    fn wire_schema(_ctx: &mut WireSchemaContext, out: &mut Vec<u8>) {
        out.push(SCHEMA_TAG_INTEGER);
        out.push(0);
        out.push(0);
        out.push(8);
    }
}

impl WireSchema for i8 {
    fn wire_schema(_ctx: &mut WireSchemaContext, out: &mut Vec<u8>) {
        out.push(SCHEMA_TAG_INTEGER);
        out.push(1);
        out.push(0);
        out.push(8);
    }
}

// `usize`/`isize` always travel as 8 native-endian bytes (via `u64`), so
// they are natives with fixed width 8; the signed flag follows the value
// domain.
impl WireSchema for usize {
    fn wire_schema(_ctx: &mut WireSchemaContext, out: &mut Vec<u8>) {
        out.push(SCHEMA_TAG_NATIVE);
        out.push(8);
        out.push(0);
        out.push(0);
        out.push(SCHEMA_NATIVE_ENDIAN);
    }
}

impl WireSchema for isize {
    fn wire_schema(_ctx: &mut WireSchemaContext, out: &mut Vec<u8>) {
        out.push(SCHEMA_TAG_NATIVE);
        out.push(8);
        out.push(1);
        out.push(0);
        out.push(SCHEMA_NATIVE_ENDIAN);
    }
}

#[cfg(test)]
mod schema_tests {
    use crate::serde::WIRE_SCHEMA_DOMAIN;
    use crate::serde::{
        WireSchema, SCHEMA_NATIVE_ENDIAN, SCHEMA_TAG_BOOL, SCHEMA_TAG_CHAR, SCHEMA_TAG_INTEGER,
        SCHEMA_TAG_NATIVE, SCHEMA_TAG_UNIT,
    };

    /// Fixed grammars emit their tag (and, for transmute primitives, the
    /// host endianness); signed and float domains stay distinguishable.
    #[test]
    fn scalar_descriptors_match_their_wire_facts() {
        assert_eq!(
            &u8::wire_schema_bytes()[WIRE_SCHEMA_DOMAIN.len()..],
            &[SCHEMA_TAG_INTEGER, 0, 0, 8]
        );
        assert_eq!(
            &i8::wire_schema_bytes()[WIRE_SCHEMA_DOMAIN.len()..],
            &[SCHEMA_TAG_INTEGER, 1, 0, 8]
        );
        assert_eq!(
            &bool::wire_schema_bytes()[WIRE_SCHEMA_DOMAIN.len()..],
            &[SCHEMA_TAG_BOOL]
        );
        assert_eq!(
            &<()>::wire_schema_bytes()[WIRE_SCHEMA_DOMAIN.len()..],
            &[SCHEMA_TAG_UNIT]
        );
        assert_eq!(
            &char::wire_schema_bytes()[WIRE_SCHEMA_DOMAIN.len()..],
            &[SCHEMA_TAG_CHAR, SCHEMA_NATIVE_ENDIAN]
        );
        assert_eq!(
            &u32::wire_schema_bytes()[WIRE_SCHEMA_DOMAIN.len()..],
            &[SCHEMA_TAG_NATIVE, 4, 0, 0, SCHEMA_NATIVE_ENDIAN]
        );
        assert_eq!(
            &i64::wire_schema_bytes()[WIRE_SCHEMA_DOMAIN.len()..],
            &[SCHEMA_TAG_NATIVE, 8, 1, 0, SCHEMA_NATIVE_ENDIAN]
        );
        assert_eq!(
            &f32::wire_schema_bytes()[WIRE_SCHEMA_DOMAIN.len()..],
            &[SCHEMA_TAG_NATIVE, 4, 0, 1, SCHEMA_NATIVE_ENDIAN]
        );
        assert_eq!(
            &usize::wire_schema_bytes()[WIRE_SCHEMA_DOMAIN.len()..],
            &[SCHEMA_TAG_NATIVE, 8, 0, 0, SCHEMA_NATIVE_ENDIAN]
        );
        // Width is a fact: u16 and u32 disagree.
        assert_ne!(
            u16::wire_schema_bytes(),
            u32::wire_schema_bytes(),
            "native width must differ",
        );
    }
}
