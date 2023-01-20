use crate::{
    error::{SerdeErr, WriteOverflowError},
    reader_writer::{BitReader, BitWrite},
    serde::Serde,
};

impl<T: Serde> Serde for Option<T> {
    fn ser(&self, writer: &mut dyn BitWrite) -> Result<(), WriteOverflowError> {
        if let Some(value) = self {
            {
                let result = writer.write_bit(true);
                if result.is_err() {
                    return result;
                }
            }
            {
                let result = value.ser(writer);
                if result.is_err() {
                    return result;
                }
            }
        } else {
            let result = writer.write_bit(false);
            if result.is_err() {
                return result;
            }
        }
        Ok(())
    }

    fn de(reader: &mut BitReader) -> Result<Option<T>, SerdeErr> {
        if reader.read_bit()? {
            Ok(Some(T::de(reader)?))
        } else {
            Ok(None)
        }
    }
}

// Tests

#[cfg(test)]
mod tests {
    use crate::{
        reader_writer::{BitReader, BitWriter},
        serde::Serde,
    };

    #[test]
    fn read_write() {
        // Write
        let mut writer = BitWriter::new();

        let in_1 = Some(123);
        let in_2: Option<f32> = None;

        in_1.ser(&mut writer).unwrap();
        in_2.ser(&mut writer).unwrap();

        let (buffer_length, buffer) = writer.flush();

        // Read

        let mut reader = BitReader::new(&buffer[..buffer_length]);

        let out_1 = Option::<u8>::de(&mut reader).unwrap();
        let out_2 = Option::<f32>::de(&mut reader).unwrap();

        assert_eq!(in_1, out_1);
        assert_eq!(in_2, out_2);
    }
}
