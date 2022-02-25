
use naia_serde::{BitReader, BitWriter, De, Ser};

#[derive(Debug, Eq, PartialEq, De, Ser)]
pub struct SomeStruct {
    some_string: String,
    some_int: i16,
    some_bool: bool,
}

#[test]
fn read_write_struct() {
    // Write
    let mut writer = BitWriter::new();

    let in_1 = SomeStruct {
        some_string: "Hello world!".to_string(),
        some_int: 42,
        some_bool: true,
    };
    let in_2 = SomeStruct {
        some_string: "Hello world!".to_string(),
        some_int: 42,
        some_bool: true,
    };

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