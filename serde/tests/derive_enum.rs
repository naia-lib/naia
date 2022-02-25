
use naia_serde::{BitReader, BitWriter, Serde};

#[derive(Debug, Eq, PartialEq, Serde)]
pub enum SomeEnum {
    Variant1,
    Variant2(bool),
    Variant3(u16, String),
    Variant4 { some_bool: bool, some_number: i8, some_string: String },
    Variant5
}

#[test]
fn read_write_enum() {
    // Write
    let mut writer = BitWriter::new();

    let in_1 = SomeEnum::Variant2(true);
    let in_2 = SomeEnum::Variant1;
    let in_3 = SomeEnum::Variant3(5851, "Hello enum!".to_string());
    let in_4 = SomeEnum::Variant4 {
        some_bool: true,
        some_number: -7,
        some_string: "Heya there enum".to_string(),
    };

    writer.write(&in_1);
    writer.write(&in_2);
    writer.write(&in_3);
    writer.write(&in_4);

    let (buffer_length, buffer) = writer.flush();

    // Read

    let mut reader = BitReader::new(buffer_length, buffer);

    let out_1 = reader.read().unwrap();
    let out_2 = reader.read().unwrap();
    let out_3 = reader.read().unwrap();
    let out_4 = reader.read().unwrap();

    assert_eq!(in_1, out_1);
    assert_eq!(in_2, out_2);
    assert_eq!(in_3, out_3);
    assert_eq!(in_4, out_4);
}