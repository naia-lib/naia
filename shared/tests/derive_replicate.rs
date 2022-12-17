mod some_protocol {
    use super::some_named_replica::NamedStringHolder;
    use super::some_tuple_replica::TupleStringHolder;
    use naia_shared::Protocolize;

    #[derive(Protocolize)]
    pub enum SomeProtocol {
        NamedStringHolder(NamedStringHolder),
        TupleStringHolder(TupleStringHolder)
    }
}

mod some_named_replica {
    use naia_shared::{Property, Replicate};

    #[derive(Replicate)]
    #[protocol_path = "super::some_protocol::SomeProtocol"]
    pub struct NamedStringHolder {
        pub string_1: Property<String>,
        pub string_2: Property<String>,
    }

    impl NamedStringHolder {
        pub fn new(string_1: &str, string_2: &str) -> Self {
            return NamedStringHolder::new_complete(string_1.to_string(), string_2.to_string());
        }
    }
}

mod some_tuple_replica {
    use naia_shared::{Property, Replicate};

    #[derive(Replicate)]
    #[protocol_path = "super::some_protocol::SomeProtocol"]
    pub struct TupleStringHolder(
        pub Property<String>,
        pub Property<String>,
    );

    impl TupleStringHolder {
        pub fn new(string_1: &str, string_2: &str) -> Self {
            return TupleStringHolder::new_complete(string_1.to_string(), string_2.to_string());
        }
    }
}

use naia_shared::{
    serde::{BitReader, BitWriter},
    FakeEntityConverter, Protocolize,
};

use some_protocol::SomeProtocol;
use some_named_replica::NamedStringHolder;
use some_tuple_replica::TupleStringHolder;

#[test]
fn read_write_named_replica() {
    // Write
    let mut writer = BitWriter::new();

    let in_1 = SomeProtocol::NamedStringHolder(NamedStringHolder::new("hello world", "goodbye world"));

    in_1.write(&mut writer, &FakeEntityConverter);

    let (buffer_length, buffer) = writer.flush();

    // Read

    let mut reader = BitReader::new(&buffer[..buffer_length]);

    let out_1 = SomeProtocol::read(&mut reader, &FakeEntityConverter)
        .expect("should deserialize correctly");

    let typed_in_1 = in_1.cast_ref::<NamedStringHolder>().unwrap();
    let typed_out_1 = out_1.cast_ref::<NamedStringHolder>().unwrap();
    assert!(typed_in_1.string_1.equals(&typed_out_1.string_1));
    assert!(typed_in_1.string_2.equals(&typed_out_1.string_2));
    assert_eq!(*typed_in_1.string_1, "hello world".to_string());
    assert_eq!(*typed_in_1.string_2, "goodbye world".to_string());
    assert_eq!(*typed_out_1.string_1, "hello world".to_string());
    assert_eq!(*typed_out_1.string_2, "goodbye world".to_string());
}

#[test]
fn read_write_tuple_replica() {
    // Write
    let mut writer = BitWriter::new();

    let in_1 = SomeProtocol::TupleStringHolder(TupleStringHolder::new("hello world", "goodbye world"));

    in_1.write(&mut writer, &FakeEntityConverter);

    let (buffer_length, buffer) = writer.flush();

    // Read

    let mut reader = BitReader::new(&buffer[..buffer_length]);

    let out_1 = SomeProtocol::read(&mut reader, &FakeEntityConverter)
        .expect("should deserialize correctly");

    let typed_in_1 = in_1.cast_ref::<TupleStringHolder>().unwrap();
    let typed_out_1 = out_1.cast_ref::<TupleStringHolder>().unwrap();
    assert!(typed_in_1.0.equals(&typed_out_1.0));
    assert!(typed_in_1.1.equals(&typed_out_1.1));
    assert_eq!(*typed_in_1.0, "hello world".to_string());
    assert_eq!(*typed_in_1.1, "goodbye world".to_string());
    assert_eq!(*typed_out_1.0, "hello world".to_string());
    assert_eq!(*typed_out_1.1, "goodbye world".to_string());
}
