
use crate::{bit_reader::BitReader, bit_writer::BitWriter, error::DeErr, traits::{De, Ser}};

macro_rules! impl_ser_de_for {
    ($impl_type:ident) => {
        impl Ser for $impl_type {
            fn ser(&self, writer: &mut BitWriter) {
                let du8 = unsafe {
                    std::mem::transmute::<&$impl_type, &[u8; std::mem::size_of::<$impl_type>()]>(&self)
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

macro_rules! test_ser_de_for {
    ($impl_type:ident, $test_name:ident) => {
        #[test]
        fn $test_name() {

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
    }
}

impl_ser_de_for!(u16);
impl_ser_de_for!(u32);
impl_ser_de_for!(u64);
impl_ser_de_for!(u128);
impl_ser_de_for!(i16);
impl_ser_de_for!(i32);
impl_ser_de_for!(i64);
impl_ser_de_for!(i128);
impl_ser_de_for!(f32);
impl_ser_de_for!(f64);

mod tests {
    use crate::{BitReader, BitWriter};
    test_ser_de_for!(u8,   test_u8);
    test_ser_de_for!(u16,  test_u16);
    test_ser_de_for!(u32,  test_u32);
    test_ser_de_for!(u64,  test_u64);
    test_ser_de_for!(u128, test_u128);
    test_ser_de_for!(usize, test_usize);
    test_ser_de_for!(i8,   test_i8);
    test_ser_de_for!(i16,  test_i16);
    test_ser_de_for!(i32,  test_i32);
    test_ser_de_for!(i64,  test_i64);
    test_ser_de_for!(i128, test_i128);
    test_ser_de_for!(isize, test_isize);
    test_ser_de_for!(f32,  test_f32);
    test_ser_de_for!(f64,  test_f64);
}

// impl Ser for usize {
//     fn ser(&self, s: &mut Vec<u8>) {
//         let u64usize = *self as u64;
//         let du8 =
//             unsafe { std::mem::transmute::<&u64, &[u8; std::mem::size_of::<u64>()]>(&u64usize) };
//         s.extend_from_slice(du8);
//     }
// }
//
// impl De for usize {
//     fn de(o: &mut usize, d: &[u8]) -> Result<usize, DeErr> {
//         let l = std::mem::size_of::<u64>();
//         if *o + l > d.len() {
//             return Err(DeErr {
//                 o: *o,
//                 l: l,
//                 s: d.len(),
//             });
//         }
//         let mut m = [0 as u64];
//         unsafe {
//             std::ptr::copy_nonoverlapping(
//                 d.as_ptr().offset(*o as isize) as *const u64,
//                 m.as_mut_ptr() as *mut u64,
//                 1,
//             )
//         }
//         *o += l;
//         Ok(m[0] as usize)
//     }
// }
//
// impl De for u8 {
//     fn de(o: &mut usize, d: &[u8]) -> Result<u8, DeErr> {
//         if *o + 1 > d.len() {
//             return Err(DeErr {
//                 o: *o,
//                 l: 1,
//                 s: d.len(),
//             });
//         }
//         let m = d[*o];
//         *o += 1;
//         Ok(m)
//     }
// }
//
// impl Ser for u8 {
//     fn ser(&self, s: &mut Vec<u8>) {
//         s.push(*self);
//     }
// }