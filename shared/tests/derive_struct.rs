mod some_struct {
    use naia_shared::{derive_serde, serde};

    #[derive(Debug)]
    #[derive_serde]
    pub struct SomeStruct {
        pub some_string: String,
        pub some_int: i16,
        pub some_bool: bool,
    }
}

use naia_shared::serde::{BitReader, BitWriter, Serde};

use some_struct::SomeStruct;

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
        some_string: "Goodbye world!".to_string(),
        some_int: -42,
        some_bool: false,
    };

    in_1.ser(&mut writer);
    in_2.ser(&mut writer);

    let (buffer_length, buffer) = writer.flush();

    // Read

    let mut reader = BitReader::new(&buffer[..buffer_length]);

    let out_1 = Serde::de(&mut reader).unwrap();
    let out_2 = Serde::de(&mut reader).unwrap();

    assert_eq!(in_1, out_1);
    assert_eq!(in_2, out_2);
}
