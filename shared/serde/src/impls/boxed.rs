use crate::{
    bit_reader::BitReader,
    bit_writer::BitWrite,
    error::SerdeErr,
    serde::{ConstBitLength, Serde},
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
