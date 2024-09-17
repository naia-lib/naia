use std::marker::PhantomData;

use crate::{
    bit_reader::BitReader,
    bit_writer::BitWrite,
    error::SerdeErr,
    serde::{ConstBitLength, Serde},
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
