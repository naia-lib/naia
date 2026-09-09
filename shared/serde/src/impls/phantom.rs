use std::marker::PhantomData;

use crate::{
    bit_reader::BitReader,
    bit_writer::BitWrite,
    error::SerdeErr,
    serde::{ConstBitLength, Serde, WireSchema, WireSchemaContext, SCHEMA_TAG_PHANTOM},
};

// Unit //

impl<T> Serde for PhantomData<T> {
    fn ser(&self, _: &mut dyn BitWrite) {}

    fn de(_: &mut BitReader) -> Result<Self, SerdeErr> {
        Ok(Self)
    }

    fn bit_length(&self) -> u32 {
        <Self as ConstBitLength>::const_bit_length()
    }
}

impl<T> ConstBitLength for PhantomData<T> {
    fn const_bit_length() -> u32 {
        0
    }
}

// tests

#[cfg(test)]
mod phantom_tests {
    use crate::{bit_reader::BitReader, bit_writer::BitWriter, serde::Serde};
    use std::marker::PhantomData;

    #[test]
    fn read_write() {
        // Write
        let mut writer = BitWriter::new();

        let in_phantom = PhantomData::<u32>;

        in_phantom.ser(&mut writer);

        let buffer = writer.to_bytes();

        //Read
        let mut reader = BitReader::new(&buffer);

        let out_phantom = Serde::de(&mut reader).unwrap();

        assert_eq!(in_phantom, out_phantom);
    }
}

// Schema descriptors //

// `PhantomData<T>` is zero-wire: it emits its tag and does not recurse
// into `T`, so a phantom parameter can never smuggle a dependency (or a
// loop) into the descriptor.
impl<T: 'static> WireSchema for PhantomData<T> {
    fn wire_schema(_ctx: &mut WireSchemaContext, out: &mut Vec<u8>) {
        out.push(SCHEMA_TAG_PHANTOM);
    }
}

#[cfg(test)]
mod schema_tests {
    use crate::serde::WIRE_SCHEMA_DOMAIN;
    use crate::serde::{WireSchema, SCHEMA_TAG_PHANTOM};
    use std::marker::PhantomData;

    /// Phantom parameters are invisible: any `T` describes identically, and
    /// a recursive `T` still terminates.
    #[test]
    fn phantom_is_zero_wire_and_never_recurses() {
        assert_eq!(
            &PhantomData::<u8>::wire_schema_bytes()[WIRE_SCHEMA_DOMAIN.len()..],
            &[SCHEMA_TAG_PHANTOM],
        );
        assert_eq!(
            PhantomData::<u8>::wire_schema_bytes(),
            PhantomData::<Vec<String>>::wire_schema_bytes(),
            "the phantom parameter must not leak in",
        );
    }
}
