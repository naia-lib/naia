// use crate::{
//     bit_reader::BitReader,
//     bit_writer::BitWriter,
//     error::DeErr,
//     traits::{De, Ser},
// };
//
// impl Ser for String {
//     fn ser(&self, writer: &mut BitWriter) {
//         let length: usize = self.len();
//         writer.write(&length);
//         let bytes = self.as_bytes();
//         for byte in bytes {
//             writer.write_byte(*byte);
//         }
//     }
// }
//
// impl De for String {
//     fn de(reader: &mut BitReader) -> Result<Self, DeErr> {
//         let length: usize = reader.read().unwrap();
//         let r = std::str::from_utf8(&d[*o..(*o + length)]).unwrap().to_string();
//         *o += length;
//         Ok(r)
//     }
// }
//
// // Tests
//
// #[cfg(test)]
// mod tests {
//     use crate::{BitReader, BitWriter};
//
//     #[test]
//     fn read_write() {
//         // Write
//         let mut writer = BitWriter::new();
//
//         let in_true_bool = true;
//         let in_false_bool = false;
//
//         writer.write(&in_true_bool);
//         writer.write(&in_false_bool);
//
//         let (buffer_length, buffer) = writer.flush();
//
//         // Read
//
//         let mut reader = BitReader::new(buffer_length, buffer);
//
//         let out_true_bool = reader.read().unwrap();
//         let out_false_bool = reader.read().unwrap();
//
//         assert_eq!(in_true_bool, out_true_bool);
//         assert_eq!(in_false_bool, out_false_bool);
//     }
// }
