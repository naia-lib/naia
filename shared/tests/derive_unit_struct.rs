mod some_struct {
    use naia_shared::derive_serde;

    #[derive(Debug)]
    #[derive_serde]
    pub struct SomeStruct;
}

use naia_shared::{BitReader, BitWriter};
use some_struct::SomeStruct;

#[test]
fn read_write_unit_struct() {
    // Write
    let mut writer = BitWriter::new();

    let in_1 = SomeStruct;

    in_1.ser(&mut writer);

    let (buffer_length, buffer) = writer.flush();

    // Read

    let mut reader = BitReader::new(buffer_length, buffer);

    let out_1 = reader.read().unwrap();

    assert_eq!(in_1, out_1);
}
