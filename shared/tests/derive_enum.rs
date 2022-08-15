mod some_enum {
    use naia_serde as serde;
    use serde::derive_serde;

    #[derive(Debug)]
    #[derive_serde]
    pub enum SomeEnum {
        Variant1,
        Variant2(bool),
        Variant3(u16, String),
        Variant4 {
            some_bool: bool,
            some_number: i8,
            some_string: String,
        },
        Variant5,
    }
}

mod some_enum_2 {
    use naia_shared::{derive_serde, serde};

    #[derive(Debug)]
    #[derive_serde]
    pub enum SomeEnum2 {
        Variant1,
        Variant2,
        Variant3,
    }
}

use naia_shared::serde::{BitReader, BitWriter, Serde};
use some_enum::SomeEnum;
use some_enum_2::SomeEnum2;

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

    in_1.ser(&mut writer);
    in_2.ser(&mut writer);
    in_3.ser(&mut writer);
    in_4.ser(&mut writer);

    let (buffer_length, buffer) = writer.flush();

    // Read

    let mut reader = BitReader::new(&buffer[..buffer_length]);

    let out_1 = Serde::de(&mut reader).unwrap();
    let out_2 = Serde::de(&mut reader).unwrap();
    let out_3 = Serde::de(&mut reader).unwrap();
    let out_4 = Serde::de(&mut reader).unwrap();

    assert_eq!(in_1, out_1);
    assert_eq!(in_2, out_2);
    assert_eq!(in_3, out_3);
    assert_eq!(in_4, out_4);
}

#[test]
fn read_write_enum_2() {
    // Write
    let mut writer = BitWriter::new();

    let in_1 = SomeEnum2::Variant2;
    let in_2 = SomeEnum2::Variant1;
    let in_3 = SomeEnum2::Variant3;

    in_1.ser(&mut writer);
    in_2.ser(&mut writer);
    in_3.ser(&mut writer);

    let (buffer_length, buffer) = writer.flush();

    // Read

    let mut reader = BitReader::new(&buffer[..buffer_length]);

    let out_1 = Serde::de(&mut reader).unwrap();
    let out_2 = Serde::de(&mut reader).unwrap();
    let out_3 = Serde::de(&mut reader).unwrap();

    assert_eq!(in_1, out_1);
    assert_eq!(in_2, out_2);
    assert_eq!(in_3, out_3);
}
