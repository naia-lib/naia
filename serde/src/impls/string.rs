use crate::{
    error::SerdeErr,
    reader_writer::{BitReader, BitWrite},
    serde::Serde,
    UnsignedInteger,
};

impl Serde for String {
    fn ser<S: BitWrite>(&self, writer: &mut S) {
        let length = UnsignedInteger::<9>::new(self.len() as u64);
        length.ser(writer);
        let bytes = self.as_bytes();
        for byte in bytes {
            writer.write_byte(*byte);
        }
    }

    fn de(reader: &mut BitReader) -> Result<Self, SerdeErr> {
        let length_int: UnsignedInteger<9> = reader.read().unwrap();
        let length_usize = length_int.get() as usize;
        let mut bytes: Vec<u8> = Vec::with_capacity(length_usize);
        for _ in 0..length_usize {
            bytes.push(reader.read_byte());
        }

        let result = std::str::from_utf8(&bytes).unwrap().to_string();
        Ok(result)
    }
}

// Tests

#[cfg(test)]
mod tests {
    use crate::{serde::Serde, reader_writer::{BitReader, BitWriter}};

    #[test]
    fn read_write() {
        // Write
        let mut writer = BitWriter::new();

        let in_1 = "Hello world!".to_string();
        let in_2 = "This is a string.".to_string();

        in_1.ser(&mut writer);
        in_2.ser(&mut writer);

        let (buffer_length, buffer) = writer.flush();

        // Read

        let mut reader = BitReader::new(buffer_length, buffer);

        let out_1: String = reader.read().unwrap();
        let out_2: String = reader.read().unwrap();

        assert_eq!(in_1, out_1);
        assert_eq!(in_2, out_2);
    }
}
