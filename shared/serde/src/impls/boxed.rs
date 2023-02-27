use crate::{
    bit_reader::BitReader,
    bit_writer::BitWrite,
    error::SerdeErr,
    serde::{ConstBitLength, Serde},
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
        return T::const_bit_length();
    }
}

// Tests

#[cfg(test)]
mod tests {
    use crate::{
        bit_reader::BitReader,
        bit_writer::BitWriter,
        serde::Serde,
    };

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
