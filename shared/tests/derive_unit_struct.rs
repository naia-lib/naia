mod some_struct {
    use naia_shared::{derive_serde, serde};

    #[derive(Debug)]
    #[derive_serde]
    pub struct SomeStruct;
}

use naia_shared::serde::{BitReader, BitWriter, Serde};
use some_struct::SomeStruct;

#[test]
fn read_write_unit_struct() {
    // Write
    let mut writer = BitWriter::new();

    let in_1 = SomeStruct;

    in_1.ser(&mut writer);

    let (buffer_length, buffer) = writer.flush();

    // Read

    let mut reader = BitReader::new(&buffer[..buffer_length]);

    let out_1 = Serde::de(&mut reader).unwrap();

    assert_eq!(in_1, out_1);
}
