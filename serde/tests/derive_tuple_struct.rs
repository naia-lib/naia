
use naia_serde::{BitReader, BitWriter, Serde};

#[derive(Debug, Eq, PartialEq, Serde)]
pub struct SomeStruct(String, i16, bool);

#[test]
fn read_write_tuple_struct() {
    // Write
    let mut writer = BitWriter::new();

    let in_1 = SomeStruct("Hello world!".to_string(), 42, true);
    let in_2 = SomeStruct("Goodbye world!".to_string(), -42, false);

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