use crate::{
    reader_writer::{BitReader, BitWriter},
    error::DeErr,
    traits::{De, Ser},
};

impl<T> Ser for Option<T>
where
    T: Ser,
{
    fn ser(&self, writer: &mut BitWriter) {
        if let Some(value) = self {
            writer.write_bit(true);
            value.ser(writer);
        } else {
            writer.write_bit(false);
        }
    }
}

impl<T> De for Option<T>
where
    T: De,
{
    fn de(reader: &mut BitReader) -> Result<Option<T>, DeErr> {
        if reader.read_bit() {
            Ok(Some(De::de(reader)?))
        } else {
            Ok(None)
        }
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

        let in_some_option = Some(123);
        let in_none_option: Option<f32> = None;

        writer.write(&in_some_option);
        writer.write(&in_none_option);

        let (buffer_length, buffer) = writer.flush();

        // Read

        let mut reader = BitReader::new(buffer_length, buffer);

        let out_some_option = reader.read().unwrap();
        let out_none_option = reader.read().unwrap();

        assert_eq!(in_some_option, out_some_option);
        assert_eq!(in_none_option, out_none_option);
    }
}
