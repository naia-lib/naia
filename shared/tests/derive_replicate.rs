mod some_protocol {
    use super::some_replica::StringHolder;
    use naia_shared::Protocolize;

    #[derive(Protocolize)]
    pub enum SomeProtocol {
        StringHolder(StringHolder),
    }
}

mod some_replica {
    use naia_shared::{Property, Replicate};

    #[derive(Replicate)]
    #[protocol_path = "super::some_protocol::SomeProtocol"]
    pub struct StringHolder {
        pub string_1: Property<String>,
        pub string_2: Property<String>,
    }

    impl StringHolder {
        pub fn new(string_1: &str, string_2: &str) -> Self {
            return StringHolder::new_complete(string_1.to_string(), string_2.to_string());
        }
    }
}

use naia_shared::{
    serde::{BitReader, BitWriter},
    FakeEntityConverter, Protocolize,
};

use some_protocol::SomeProtocol;
use some_replica::StringHolder;

#[test]
fn read_write_protocol() {
    // Write
    let mut writer = BitWriter::new();

    let in_1 = SomeProtocol::StringHolder(StringHolder::new("hello world", "goodbye world"));

    in_1.write(&mut writer, &FakeEntityConverter);

    let (buffer_length, buffer) = writer.flush();

    // Read

    let mut reader = BitReader::new(&buffer[..buffer_length]);

    let out_1 = SomeProtocol::read(&mut reader, &FakeEntityConverter);

    let typed_in_1 = in_1.cast_ref::<StringHolder>().unwrap();
    let typed_out_1 = out_1.cast_ref::<StringHolder>().unwrap();
    assert!(typed_in_1.string_1.equals(&typed_out_1.string_1));
    assert!(typed_in_1.string_2.equals(&typed_out_1.string_2));
    assert_eq!(*typed_in_1.string_1, "hello world".to_string());
    assert_eq!(*typed_in_1.string_2, "goodbye world".to_string());
    assert_eq!(*typed_out_1.string_1, "hello world".to_string());
    assert_eq!(*typed_out_1.string_2, "goodbye world".to_string());
}
