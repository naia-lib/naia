use crate::{
    bit_reader::BitReader,
    bit_writer::BitWriter,
    error::DeErr,
    traits::{De, Ser},
};

impl Ser for char {
    fn ser(&self, writer: &mut BitWriter) {
        let u32char = *self as u32;
        let bytes = unsafe { std::mem::transmute::<&u32, &[u8; 4]>(&u32char) };
        for byte in bytes {
            writer.write_byte(*byte);
        }
    }
}

impl De for char {
    fn de(reader: &mut BitReader) -> Result<Self, DeErr> {
        let mut bytes = [0_u8; 4];
        for index in 0..4 {
            bytes[index] = reader.read_byte();
        }
        let mut container = [0 as u32];
        unsafe {
            std::ptr::copy_nonoverlapping(
                bytes.as_ptr().offset(0 as isize) as *const u32,
                container.as_mut_ptr() as *mut u32,
                1,
            )
        }

        return if let Some(inner_char) = char::from_u32(container[0]) {
            Ok(inner_char)
        } else {
            Err(DeErr {})
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

        let in_oh_char = 'O';
        let in_bang_char = '!';

        writer.write(&in_oh_char);
        writer.write(&in_bang_char);

        let (buffer_length, buffer) = writer.flush();

        // Read

        let mut reader = BitReader::new(buffer_length, buffer);

        let out_oh_char = reader.read().unwrap();
        let out_bang_char = reader.read().unwrap();

        assert_eq!(in_oh_char, out_oh_char);
        assert_eq!(in_bang_char, out_bang_char);
    }
}
