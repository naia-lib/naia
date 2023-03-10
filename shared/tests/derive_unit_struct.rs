mod some_struct {
    use naia_shared::Serde;

    #[derive(Clone, Debug, PartialEq, Serde)]
    pub struct SomeStruct;
}

use naia_shared::{BitReader, BitWriter, Serde};
use some_struct::SomeStruct;

#[test]
fn read_write_unit_struct() {
    // Write
    let mut writer = BitWriter::new();

    let in_1 = SomeStruct;

    in_1.ser(&mut writer);

    let bytes = writer.to_bytes();

    // Read

    let mut reader = BitReader::new(&bytes);

    let out_1 = Serde::de(&mut reader).unwrap();

    assert_eq!(in_1, out_1);
}
