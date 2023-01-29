mod some_protocol {
    use super::some_entity_replica::EntityPropertyHolder;
    use super::some_named_replica::NamedStringHolder;
    use super::some_nonreplicated_replica::MixedReplicationHolder;
    use super::some_tuple_replica::TupleStringHolder;
    use super::some_unit_replica::UnitHolder;
    use naia_shared::Protocolize;

    #[derive(Protocolize)]
    pub enum SomeProtocol {
        NamedStringHolder(NamedStringHolder),
        TupleStringHolder(TupleStringHolder),
        EntityPropertyHolder(EntityPropertyHolder),
        UnitHolder(UnitHolder),
        MixedReplicationHolder(MixedReplicationHolder),
    }
}

mod some_unit_replica {
    use naia_shared::Replicate;

    #[derive(Replicate)]
    #[protocol_path = "super::some_protocol::SomeProtocol"]
    pub struct UnitHolder;

    impl UnitHolder {
        pub fn new() -> Self {
            return UnitHolder::new_complete();
        }
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
    pub struct TupleStringHolder(pub Property<String>, pub Property<String>);

    impl TupleStringHolder {
        pub fn new(string_1: &str, string_2: &str) -> Self {
            return TupleStringHolder::new_complete(string_1.to_string(), string_2.to_string());
        }
    }
}

mod some_entity_replica {
    use naia_shared::{EntityProperty, Replicate};
    #[derive(Replicate)]
    #[protocol_path = "super::some_protocol::SomeProtocol"]
    pub struct EntityPropertyHolder {
        pub entity_1: EntityProperty,
    }
    impl EntityPropertyHolder {
        pub fn new() -> Self {
            return EntityPropertyHolder::new_complete();
        }
    }
}

mod some_nonreplicated_replica {
    use naia_shared::{Property, Replicate};

    #[derive(Replicate)]
    #[protocol_path = "super::some_protocol::SomeProtocol"]
    pub struct MixedReplicationHolder {
        pub string_1: Property<String>,
        pub string_2: String,
    }

    impl MixedReplicationHolder {
        pub fn new(string_1: &str, string_2: &str) -> Self {
            return MixedReplicationHolder::new_complete(
                string_1.to_string(),
                string_2.to_string(),
            );
        }
    }
}

use naia_shared::{
    serde::{BitReader, BitWriter},
    BigMapKey, EntityDoesNotExistError, EntityHandle, EntityHandleConverter, FakeEntityConverter,
    NetEntity, NetEntityHandleConverter, Protocolize, ReplicateSafe,
};

use some_entity_replica::EntityPropertyHolder;
use some_named_replica::NamedStringHolder;
use some_nonreplicated_replica::MixedReplicationHolder;
use some_protocol::SomeProtocol;
use some_tuple_replica::TupleStringHolder;
use some_unit_replica::UnitHolder;

#[test]
fn read_write_unit_replica() {
    // Write
    let mut writer = BitWriter::new();

    let in_1 = SomeProtocol::UnitHolder(UnitHolder::new());

    in_1.write(&mut writer, &FakeEntityConverter);

    let (buffer_length, buffer) = writer.flush();

    // Read

    let mut reader = BitReader::new(&buffer[..buffer_length]);

    let out_1 = SomeProtocol::read(&mut reader, &FakeEntityConverter)
        .expect("should deserialize correctly");

    let _typed_in_1 = in_1.cast_ref::<UnitHolder>().unwrap();
    let _typed_out_1 = out_1.cast_ref::<UnitHolder>().unwrap();
}

#[test]
fn read_write_named_replica() {
    // Write
    let mut writer = BitWriter::new();

    let in_1 =
        SomeProtocol::NamedStringHolder(NamedStringHolder::new("hello world", "goodbye world"));

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

    let in_1 =
        SomeProtocol::TupleStringHolder(TupleStringHolder::new("hello world", "goodbye world"));

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

#[test]
fn read_write_entity_replica() {
    pub struct TestEntityConverter;

    impl EntityHandleConverter<u64> for TestEntityConverter {
        fn handle_to_entity(&self, entity_handle: &EntityHandle) -> u64 {
            entity_handle.to_u64()
        }
        fn entity_to_handle(&self, entity: &u64) -> Result<EntityHandle, EntityDoesNotExistError> {
            Ok(EntityHandle::from_u64(*entity))
        }
    }
    impl NetEntityHandleConverter for TestEntityConverter {
        fn handle_to_net_entity(&self, entity_handle: &EntityHandle) -> NetEntity {
            NetEntity::from(entity_handle.to_u64() as u16)
        }
        fn net_entity_to_handle(
            &self,
            net_entity: &NetEntity,
        ) -> Result<EntityHandle, EntityDoesNotExistError> {
            let net_entity_u16: u16 = (*net_entity).into();
            Ok(EntityHandle::from_u64(net_entity_u16 as u64))
        }
    }
    // Write
    let mut writer = BitWriter::new();
    let mut in_1 = EntityPropertyHolder::new();
    in_1.entity_1.set(&TestEntityConverter, &1);
    let in_1 = SomeProtocol::EntityPropertyHolder(in_1);
    in_1.write(&mut writer, &TestEntityConverter);
    let (buffer_length, buffer) = writer.flush();
    // Read
    let mut reader = BitReader::new(&buffer[..buffer_length]);
    let out_1 = SomeProtocol::read(&mut reader, &TestEntityConverter)
        .expect("should deserialize correctly");
    let typed_in_1 = in_1.cast_ref::<EntityPropertyHolder>().unwrap();
    let typed_out_1 = out_1.cast_ref::<EntityPropertyHolder>().unwrap();
    assert!(typed_in_1.entity_1.equals(&typed_out_1.entity_1));
    let entity_handles = Vec::<EntityHandle>::from([EntityHandle::from_u64(1)]);
    assert_eq!(typed_in_1.entities(), entity_handles);
    assert_eq!(typed_out_1.entities(), entity_handles);
    assert_eq!(typed_in_1.entity_1.get(&TestEntityConverter).unwrap(), 1);
    assert_eq!(typed_in_1.entity_1.handle().unwrap().to_u64(), 1);
    assert_eq!(typed_out_1.entity_1.get(&TestEntityConverter).unwrap(), 1);
    assert_eq!(typed_out_1.entity_1.handle().unwrap().to_u64(), 1);
}

#[test]
fn read_write_nonreplicated_replica() {
    // Write
    let mut writer = BitWriter::new();

    let in_1 = SomeProtocol::MixedReplicationHolder(MixedReplicationHolder::new(
        "hello world",
        "goodbye world",
    ));

    in_1.write(&mut writer, &FakeEntityConverter);

    let (buffer_length, buffer) = writer.flush();

    // Read

    let mut reader = BitReader::new(&buffer[..buffer_length]);

    let out_1 = SomeProtocol::read(&mut reader, &FakeEntityConverter)
        .expect("should deserialize correctly");

    let typed_in_1 = in_1.cast_ref::<MixedReplicationHolder>().unwrap();
    let typed_out_1 = out_1.cast_ref::<MixedReplicationHolder>().unwrap();
    assert!(typed_in_1.string_1.equals(&typed_out_1.string_1));
    assert_eq!(*typed_in_1.string_1, "hello world".to_string());
    assert_eq!(*typed_in_1.string_2, "goodbye world".to_string());
    assert_eq!(*typed_out_1.string_1, "hello world".to_string());
    assert_eq!(*typed_out_1.string_2, "".to_string());
}
