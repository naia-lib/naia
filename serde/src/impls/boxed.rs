use crate::{
    reader_writer::{BitReader, BitWriter},
    error::DeErr,
    traits::{De, Ser},
};

impl<T> Ser for Box<T>
    where
        T: Ser,
{
    fn ser(&self, writer: &mut BitWriter) {
        (**self).ser(writer)
    }
}

impl<T> De for Box<T>
    where
        T: De,
{
    fn de(reader: &mut BitReader) -> Result<Box<T>, DeErr> {
        Ok(Box::new(De::de(reader)?))
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

        let in_1 = Box::new(123);
        let in_2 = Box::new(true);

        writer.write(&in_1);
        writer.write(&in_2);

        let (buffer_length, buffer) = writer.flush();

        // Read

        let mut reader = BitReader::new(buffer_length, buffer);

        let out_1 = reader.read().unwrap();
        let out_2 = reader.read().unwrap();

        assert_eq!(in_1, out_1);
        assert_eq!(in_2, out_2);
    }
}
