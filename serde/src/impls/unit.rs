use crate::{
    bit_reader::BitReader,
    bit_writer::BitWriter,
    error::DeErr,
    traits::{De, Ser},
};

impl Ser for () {
    fn ser(&self, _: &mut BitWriter) {}
}

impl De for () {
    fn de(_: &mut BitReader) -> Result<Self, DeErr> {
        Ok(())
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

        let in_unit = ();

        writer.write(&in_unit);

        let (buffer_length, buffer) = writer.flush();

        // Read
        let mut reader = BitReader::new(buffer_length, buffer);

        let out_unit = reader.read().unwrap();

        assert_eq!(in_unit, out_unit);
    }
}
