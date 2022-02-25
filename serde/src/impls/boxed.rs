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

        let in_boxed_int = Box::new(123);
        let in_boxed_bool = Box::new(true);

        writer.write(&in_boxed_int);
        writer.write(&in_boxed_bool);

        let (buffer_length, buffer) = writer.flush();

        // Read

        let mut reader = BitReader::new(buffer_length, buffer);

        let out_boxed_int = reader.read().unwrap();
        let out_boxed_bool = reader.read().unwrap();

        assert_eq!(in_boxed_int, out_boxed_int);
        assert_eq!(in_boxed_bool, out_boxed_bool);
    }
}
