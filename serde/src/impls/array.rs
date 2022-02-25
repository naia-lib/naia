use crate::{reader_writer::{BitReader, BitWriter}, error::DeErr, traits::{De, Ser}};

impl<T: Ser> Ser for [T] {
    fn ser(&self, writer: &mut BitWriter) {
        for item in self {
            writer.write(item);
        }
    }
}

impl<T: Ser, const N: usize> Ser for [T; N] {
    fn ser(&self, writer: &mut BitWriter) {
        for item in self {
            writer.write(item);
        }
    }
}

impl<T: De, const N: usize> De for [T; N] {
    fn de(reader: &mut BitReader) -> Result<Self, DeErr> {
        unsafe{
            let mut to = std::mem::MaybeUninit::<[T; N]>::uninit();
            let top: *mut T = std::mem::transmute(&mut to);
            for c in 0..N {
                top.add(c).write(De::de(reader)?);
            }
            Ok(to.assume_init())
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

        let in_1: [i32; 4] = [5, 11, 52, 8];
        let in_2: [bool; 3] = [true, false, true];

        writer.write(&in_1);
        writer.write(&in_2);

        let (buffer_length, buffer) = writer.flush();

        // Read

        let mut reader = BitReader::new(buffer_length, buffer);

        let out_1: [i32; 4] = reader.read().unwrap();
        let out_2: [bool; 3] = reader.read().unwrap();

        assert_eq!(in_1, out_1);
        assert_eq!(in_2, out_2);
    }
}