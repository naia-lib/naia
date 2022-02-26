mod some_struct {
    use naia_shared::derive_serde;

    #[derive(Debug)]
    #[derive_serde]
    pub struct SomeStruct(pub String, pub i16, pub bool);
}

use naia_shared::{BitReader, BitWriter};
use some_struct::SomeStruct;

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
