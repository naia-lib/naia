/// REGRESSION TEST FOR BUG #6: EntityProperty redirect panic
///
/// THE BUG: EntityProperty applied redirects in `new_read()` but NOT in `waiting_complete()`.
/// This caused a panic when an Edge component tried to reference a Vertex that had been
/// migrated, because the RemoteEntity ID changed but the EntityProperty wasn't updated.
///
/// THE SCENARIO (from Cyberlith Editor):
/// 1. Client A creates Vertex, delegates to server (entity migrates)
/// 2. Client B creates Edge referencing Vertex  
/// 3. Edge arrives at Client A with old Vertex entity ID
/// 4. Client A tries to complete EntityProperty → PANIC!
///
/// This test would have caught the bug if it existed before production.
use naia_shared::{
    BigMapKey, BitReader, BitWriter, EntityProperty, FakeEntityConverter, GlobalEntity, HostType,
    LocalEntityAndGlobalEntityConverter, LocalEntityMap, OwnedLocalEntity, RemoteEntity, Serde,
};

/// Test that EntityProperty::new_read applies redirects correctly
#[test]
fn bug_06_entity_property_new_read_redirects() {
    let mut entity_map = LocalEntityMap::new(HostType::Client);

    // Setup: Entity that was migrated
    let global_entity = GlobalEntity::from_u64(2001);
    let old_remote = RemoteEntity::new(600);
    let new_remote = RemoteEntity::new(601);

    entity_map.insert_with_remote_entity(global_entity, new_remote);
    entity_map.install_entity_redirect(
        OwnedLocalEntity::Remote(old_remote.value()),
        OwnedLocalEntity::Remote(new_remote.value()),
    );

    // Serialize EntityProperty with OLD entity ID (before migration)
    let mut writer = BitWriter::new();
    true.ser(&mut writer); // exists = true
    OwnedLocalEntity::Remote(old_remote.value()).ser(&mut writer);

    // Deserialize - should apply redirect
    let bytes = writer.to_bytes();
    let mut reader = BitReader::new(&bytes);
    let converter = entity_map.entity_converter();

    let result = EntityProperty::new_read(&mut reader, converter);

    assert!(
        result.is_ok(),
        "EntityProperty deserialization should succeed with redirect applied"
    );

    let property = result.unwrap();
    assert_eq!(
        property.get_inner(),
        Some(global_entity),
        "EntityProperty should resolve to correct GlobalEntity via redirect"
    );
}

/// CRITICAL TEST: EntityProperty::waiting_complete with redirects
/// THIS IS THE TEST THAT WAS MISSING AND WOULD HAVE CAUGHT BUG #6
#[test]
fn bug_06_entity_property_waiting_complete_redirects() {
    let mut entity_map = LocalEntityMap::new(HostType::Client);

    // Scenario: Vertex was migrated, Edge references old Vertex ID
    let vertex_global = GlobalEntity::from_u64(2002);
    let vertex_old_remote = RemoteEntity::new(602);
    let vertex_new_remote = RemoteEntity::new(603);

    entity_map.insert_with_remote_entity(vertex_global, vertex_new_remote);
    entity_map.install_entity_redirect(
        OwnedLocalEntity::Remote(vertex_old_remote.value()),
        OwnedLocalEntity::Remote(vertex_new_remote.value()),
    );

    // Create EntityProperty in "waiting" state with old entity ID
    let mut writer = BitWriter::new();
    true.ser(&mut writer);
    OwnedLocalEntity::Remote(vertex_old_remote.value()).ser(&mut writer);

    let bytes = writer.to_bytes();
    let mut reader = BitReader::new(&bytes);
    let converter = entity_map.entity_converter();
    let mut property = EntityProperty::new_read(&mut reader, converter).unwrap();

    // Complete the waiting property - THIS IS WHERE BUG #6 MANIFESTED
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        property.waiting_complete(converter);
        property
    }));

    assert!(
        result.is_ok(),
        "BUG #6: waiting_complete() panicked with 'Could not convert RemoteEntity to GlobalEntity!'. \
         This test reproduces the exact production bug from Cyberlith Editor."
    );

    // Check that the property was completed successfully
    let property = result.unwrap();
    assert_eq!(
        property.get_inner(),
        Some(vertex_global),
        "Completed EntityProperty should resolve to correct GlobalEntity after redirect"
    );
}

/// Test that FakeEntityConverter doesn't apply redirects (for reference)
#[test]
fn bug_06_fake_converter_no_redirects() {
    let fake_converter = FakeEntityConverter;
    let entity = OwnedLocalEntity::Remote(400);
    let redirected = fake_converter.apply_entity_redirect(&entity);

    assert_eq!(
        entity, redirected,
        "FakeEntityConverter should be a no-op for redirects"
    );
}

/// Test EntityProperty with multiple migration scenarios
#[test]
fn bug_06_entity_property_complex_migration() {
    let mut entity_map = LocalEntityMap::new(HostType::Client);

    // Setup: Multiple entities with redirects
    let entity1 = GlobalEntity::from_u64(3001);
    let entity1_old = RemoteEntity::new(700);
    let entity1_new = RemoteEntity::new(701);

    let entity2 = GlobalEntity::from_u64(3002);
    let entity2_old = RemoteEntity::new(702);
    let entity2_new = RemoteEntity::new(703);

    entity_map.insert_with_remote_entity(entity1, entity1_new);
    entity_map.insert_with_remote_entity(entity2, entity2_new);

    entity_map.install_entity_redirect(
        OwnedLocalEntity::Remote(entity1_old.value()),
        OwnedLocalEntity::Remote(entity1_new.value()),
    );
    entity_map.install_entity_redirect(
        OwnedLocalEntity::Remote(entity2_old.value()),
        OwnedLocalEntity::Remote(entity2_new.value()),
    );

    // Test both entities resolve correctly
    for (old_id, expected_global) in [(entity1_old, entity1), (entity2_old, entity2)] {
        let mut writer = BitWriter::new();
        true.ser(&mut writer);
        OwnedLocalEntity::Remote(old_id.value()).ser(&mut writer);

        let bytes = writer.to_bytes();
        let mut reader = BitReader::new(&bytes);
        let converter = entity_map.entity_converter();
        let property = EntityProperty::new_read(&mut reader, converter).unwrap();

        assert_eq!(
            property.get_inner(),
            Some(expected_global),
            "Each migrated entity should resolve correctly"
        );
    }
}
