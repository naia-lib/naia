
use naia_serde::{BitReader, BitWriter, De, Ser};

#[derive(Debug, Eq, PartialEq, De, Ser)]
pub struct SomeStruct;

#[test]
fn read_write_unit_struct() {
    // Write
    let mut writer = BitWriter::new();

    let in_1 = SomeStruct;

    writer.write(&in_1);

    let (buffer_length, buffer) = writer.flush();

    // Read

    let mut reader = BitReader::new(buffer_length, buffer);

    let out_1 = reader.read().unwrap();

    assert_eq!(in_1, out_1);
}