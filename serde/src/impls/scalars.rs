use crate::{
    bit_reader::BitReader,
    bit_writer::BitWriter,
    error::DeErr,
    traits::{De, Ser},
};

// Unit //

impl Ser for () {
    fn ser(&self, _: &mut BitWriter) {}
}

impl De for () {
    fn de(_: &mut BitReader) -> Result<Self, DeErr> {
        Ok(())
    }
}

// tests

#[cfg(test)]
mod unit_tests {
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

// Boolean //

impl Ser for bool {
    fn ser(&self, writer: &mut BitWriter) {
        writer.write_bit(*self);
    }
}

impl De for bool {
    fn de(reader: &mut BitReader) -> Result<Self, DeErr> {
        Ok(reader.read_bit())
    }
}

// tests

#[cfg(test)]
mod bool_tests {
    use crate::{BitReader, BitWriter};

    #[test]
    fn read_write() {
        // Write
        let mut writer = BitWriter::new();

        let in_true_bool = true;
        let in_false_bool = false;

        writer.write(&in_true_bool);
        writer.write(&in_false_bool);

        let (buffer_length, buffer) = writer.flush();

        // Read

        let mut reader = BitReader::new(buffer_length, buffer);

        let out_true_bool = reader.read().unwrap();
        let out_false_bool = reader.read().unwrap();

        assert_eq!(in_true_bool, out_true_bool);
        assert_eq!(in_false_bool, out_false_bool);
    }
}

// Characters //

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

// tests

#[cfg(test)]
mod char_tests {
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

// Integers & Floating-point Numbers //

macro_rules! impl_ser_de_for {
    ($impl_type:ident) => {
        impl Ser for $impl_type {
            fn ser(&self, writer: &mut BitWriter) {
                let du8 = unsafe {
                    std::mem::transmute::<&$impl_type, &[u8; std::mem::size_of::<$impl_type>()]>(
                        &self,
                    )
                };
                for byte in du8 {
                    writer.write_byte(*byte);
                }
            }
        }

        impl De for $impl_type {
            fn de(reader: &mut BitReader) -> Result<$impl_type, DeErr> {
                const BYTES_LENGTH: usize = std::mem::size_of::<$impl_type>();
                let mut byte_array = [0_u8; BYTES_LENGTH];
                for index in 0..BYTES_LENGTH {
                    byte_array[index] = reader.read_byte();
                }
                let mut container = [0 as $impl_type];
                unsafe {
                    std::ptr::copy_nonoverlapping(
                        byte_array.as_ptr() as *const $impl_type,
                        container.as_mut_ptr() as *mut $impl_type,
                        1,
                    )
                }
                Ok(container[0])
            }
        }
    };
}

// number primitives
impl_ser_de_for!(u16);
impl_ser_de_for!(u32);
impl_ser_de_for!(u64);
impl_ser_de_for!(i16);
impl_ser_de_for!(i32);
impl_ser_de_for!(i64);
impl_ser_de_for!(f32);
impl_ser_de_for!(f64);

// u8
impl Ser for u8 {
    fn ser(&self, writer: &mut BitWriter) {
        writer.write_byte(*self);
    }
}

impl De for u8 {
    fn de(reader: &mut BitReader) -> Result<u8, DeErr> {
        Ok(reader.read_byte())
    }
}

// i8
impl Ser for i8 {
    fn ser(&self, writer: &mut BitWriter) {
        let du8 = unsafe { std::mem::transmute::<&i8, &u8>(&self) };
        writer.write_byte(*du8);
    }
}

impl De for i8 {
    fn de(reader: &mut BitReader) -> Result<i8, DeErr> {
        let byte = [reader.read_byte()];
        let mut container = [0 as i8];
        unsafe {
            std::ptr::copy_nonoverlapping(
                byte.as_ptr() as *const i8,
                container.as_mut_ptr() as *mut i8,
                1,
            )
        }
        Ok(container[0])
    }
}

// usize
impl Ser for usize {
    fn ser(&self, writer: &mut BitWriter) {
        let u64usize = *self as u64;
        let du8 = unsafe { std::mem::transmute::<&u64, &[u8; 8]>(&u64usize) };
        for byte in du8 {
            writer.write_byte(*byte);
        }
    }
}

impl De for usize {
    fn de(reader: &mut BitReader) -> Result<usize, DeErr> {
        let mut byte_array = [0_u8; 8];
        for index in 0..8 {
            byte_array[index] = reader.read_byte();
        }
        let mut container = [0 as u64];
        unsafe {
            std::ptr::copy_nonoverlapping(
                byte_array.as_ptr().offset(0 as isize) as *const u64,
                container.as_mut_ptr() as *mut u64,
                1,
            )
        }
        Ok(container[0] as usize)
    }
}

// isize
impl Ser for isize {
    fn ser(&self, writer: &mut BitWriter) {
        let u64usize = *self as u64;
        let du8 = unsafe { std::mem::transmute::<&u64, &[u8; 8]>(&u64usize) };
        for byte in du8 {
            writer.write_byte(*byte);
        }
    }
}

impl De for isize {
    fn de(reader: &mut BitReader) -> Result<isize, DeErr> {
        let mut byte_array = [0_u8; 8];
        for index in 0..8 {
            byte_array[index] = reader.read_byte();
        }
        let mut container = [0 as u64];
        unsafe {
            std::ptr::copy_nonoverlapping(
                byte_array.as_ptr().offset(0 as isize) as *const u64,
                container.as_mut_ptr() as *mut u64,
                1,
            )
        }
        Ok(container[0] as isize)
    }
}

// tests

macro_rules! test_ser_de_for {
    ($impl_type:ident, $test_name:ident) => {
        #[test]
        fn $test_name() {
            use crate::{BitReader, BitWriter};

            // Write
            let mut writer = BitWriter::new();

            let first: $impl_type = 123 as $impl_type;

            writer.write(&first);

            let (buffer_length, buffer) = writer.flush();

            // Read
            let mut reader = BitReader::new(buffer_length, buffer);

            let last = reader.read().unwrap();

            assert_eq!(first, last);
        }
    };
}

mod number_tests {
    test_ser_de_for!(u8, test_u8);
    test_ser_de_for!(u16, test_u16);
    test_ser_de_for!(u32, test_u32);
    test_ser_de_for!(u64, test_u64);
    test_ser_de_for!(usize, test_usize);
    test_ser_de_for!(i8, test_i8);
    test_ser_de_for!(i16, test_i16);
    test_ser_de_for!(i32, test_i32);
    test_ser_de_for!(i64, test_i64);
    test_ser_de_for!(isize, test_isize);
    test_ser_de_for!(f32, test_f32);
    test_ser_de_for!(f64, test_f64);
}
