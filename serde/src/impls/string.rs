use crate::{
    error::SerdeErr,
    reader_writer::{BitReader, BitWriter},
    serde::Serde,
    UnsignedInteger,
};

impl Serde for String {
    fn ser(&self, writer: &mut BitWriter) {
        let length = UnsignedInteger::<9>::new(self.len() as u64);
        writer.write(&length);
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
    use crate::{BitReader, BitWriter};

    #[test]
    fn read_write() {
        // Write
        let mut writer = BitWriter::new();

        let in_1 = "Hello world!".to_string();
        let in_2 = "This is a string.".to_string();

        writer.write(&in_1);
        writer.write(&in_2);

        let (buffer_length, buffer) = writer.flush();

        // Read

        let mut reader = BitReader::new(buffer_length, buffer);

        let out_1: String = reader.read().unwrap();
        let out_2: String = reader.read().unwrap();

        assert_eq!(in_1, out_1);
        assert_eq!(in_2, out_2);
    }
}
