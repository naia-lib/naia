mod some_unit_replica {
    use naia_shared::Replicate;

    #[derive(Replicate)]
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
    BigMapKey, BitReader, BitWriter, EntityDoesNotExistError, EntityHandle, EntityHandleConverter,
    FakeEntityConverter, NetEntityHandleConverter, OwnedNetEntity, Protocol, Replicate,
};

use some_entity_replica::EntityPropertyHolder;
use some_named_replica::NamedStringHolder;
use some_nonreplicated_replica::MixedReplicationHolder;
use some_tuple_replica::TupleStringHolder;
use some_unit_replica::UnitHolder;

#[test]
fn read_write_unit_replica() {
    // Protocol
    let protocol = Protocol::builder().add_component::<UnitHolder>().build();
    let component_kinds = protocol.component_kinds;

    // Write
    let mut writer = BitWriter::new();

    let in_1 = UnitHolder::new();

    in_1.write(&component_kinds, &mut writer, &FakeEntityConverter);

    let (buffer_length, buffer) = writer.to_bytes();

    // Read

    let mut reader = BitReader::new(&buffer[..buffer_length]);

    let out_1 = component_kinds
        .read(&mut reader, &FakeEntityConverter)
        .expect("should deserialize correctly");

    let _typed_out_1 = out_1.to_boxed_any().downcast_ref::<UnitHolder>().unwrap();
}

#[test]
fn read_write_named_replica() {
    // Protocol
    let protocol = Protocol::builder()
        .add_component::<NamedStringHolder>()
        .build();
    let component_kinds = protocol.component_kinds;

    // Write
    let mut writer = BitWriter::new();

    let in_1 = NamedStringHolder::new("hello world", "goodbye world");

    in_1.write(&component_kinds, &mut writer, &FakeEntityConverter);

    let (buffer_length, buffer) = writer.to_bytes();

    // Read

    let mut reader = BitReader::new(&buffer[..buffer_length]);

    let out_1 = component_kinds
        .read(&mut reader, &FakeEntityConverter)
        .expect("should deserialize correctly")
        .to_boxed_any();

    let typed_out_1 = out_1.downcast_ref::<NamedStringHolder>().unwrap();
    assert!(in_1.string_1.equals(&typed_out_1.string_1));
    assert!(in_1.string_2.equals(&typed_out_1.string_2));
    assert_eq!(*in_1.string_1, "hello world".to_string());
    assert_eq!(*in_1.string_2, "goodbye world".to_string());
    assert_eq!(*typed_out_1.string_1, "hello world".to_string());
    assert_eq!(*typed_out_1.string_2, "goodbye world".to_string());
}

#[test]
fn read_write_tuple_replica() {
    // Protocol
    let protocol = Protocol::builder()
        .add_component::<TupleStringHolder>()
        .build();
    let component_kinds = protocol.component_kinds;

    // Write
    let mut writer = BitWriter::new();

    let in_1 = TupleStringHolder::new("hello world", "goodbye world");

    in_1.write(&component_kinds, &mut writer, &FakeEntityConverter);

    let (buffer_length, buffer) = writer.to_bytes();

    // Read

    let mut reader = BitReader::new(&buffer[..buffer_length]);

    let out_1 = component_kinds
        .read(&mut reader, &FakeEntityConverter)
        .expect("should deserialize correctly")
        .to_boxed_any();

    let typed_out_1 = out_1.downcast_ref::<TupleStringHolder>().unwrap();
    assert!(in_1.0.equals(&typed_out_1.0));
    assert!(in_1.1.equals(&typed_out_1.1));
    assert_eq!(*in_1.0, "hello world".to_string());
    assert_eq!(*in_1.1, "goodbye world".to_string());
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
        fn handle_to_net_entity(&self, entity_handle: &EntityHandle) -> OwnedNetEntity {
            OwnedNetEntity::from(entity_handle.to_u64() as u16)
        }
        fn net_entity_to_handle(
            &self,
            net_entity: &OwnedNetEntity,
        ) -> Result<EntityHandle, EntityDoesNotExistError> {
            let net_entity_u16: u16 = (*net_entity).into();
            Ok(EntityHandle::from_u64(net_entity_u16 as u64))
        }
    }

    // Protocol
    let protocol = Protocol::builder()
        .add_component::<EntityPropertyHolder>()
        .build();
    let component_kinds = protocol.component_kinds;

    // Write
    let mut writer = BitWriter::new();
    let mut in_1 = EntityPropertyHolder::new();
    in_1.entity_1.set(&TestEntityConverter, &1);
    in_1.write(&component_kinds, &mut writer, &TestEntityConverter);
    let (buffer_length, buffer) = writer.to_bytes();

    // Read
    let mut reader = BitReader::new(&buffer[..buffer_length]);
    let out_1 = component_kinds
        .read(&mut reader, &TestEntityConverter)
        .expect("should deserialize correctly")
        .to_boxed_any();

    let typed_out_1 = out_1.downcast_ref::<EntityPropertyHolder>().unwrap();
    assert!(in_1.entity_1.equals(&typed_out_1.entity_1));
    let entity_handles = Vec::<EntityHandle>::from([EntityHandle::from_u64(1)]);
    assert_eq!(in_1.entities(), entity_handles);
    assert_eq!(typed_out_1.entities(), entity_handles);
    assert_eq!(in_1.entity_1.get(&TestEntityConverter).unwrap(), 1);
    assert_eq!(in_1.entity_1.handle().unwrap().to_u64(), 1);
    assert_eq!(typed_out_1.entity_1.get(&TestEntityConverter).unwrap(), 1);
    assert_eq!(typed_out_1.entity_1.handle().unwrap().to_u64(), 1);
}

#[test]
fn read_write_nonreplicated_replica() {
    // Protocol
    let protocol = Protocol::builder()
        .add_component::<MixedReplicationHolder>()
        .build();
    let component_kinds = protocol.component_kinds;

    // Write
    let mut writer = BitWriter::new();

    let in_1 = MixedReplicationHolder::new("hello world", "goodbye world");

    in_1.write(&component_kinds, &mut writer, &FakeEntityConverter);

    let (buffer_length, buffer) = writer.to_bytes();

    // Read

    let mut reader = BitReader::new(&buffer[..buffer_length]);

    let out_1 = component_kinds
        .read(&mut reader, &FakeEntityConverter)
        .expect("should deserialize correctly")
        .to_boxed_any();

    let typed_out_1 = out_1.downcast_ref::<MixedReplicationHolder>().unwrap();
    assert!(in_1.string_1.equals(&typed_out_1.string_1));
    assert_eq!(*in_1.string_1, "hello world".to_string());
    assert_eq!(*in_1.string_2, "goodbye world".to_string());
    assert_eq!(*typed_out_1.string_1, "hello world".to_string());
    assert_eq!(*typed_out_1.string_2, "".to_string());
}
