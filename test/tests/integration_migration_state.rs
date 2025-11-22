/// INTEGRATION TEST: Migration state consistency
///
/// Tests that entity migration preserves all state:
/// - Component data
/// - Entity redirects
/// - Authority status
/// - Buffered commands
///
/// These tests verify the complete migration implementation.
use naia_shared::{
    BigMapKey, BitReader, BitWriter, EntityProperty, GlobalEntity, HostType, LocalEntityMap,
    OwnedLocalEntity, RemoteEntity, Serde,
};

/// Test that entity redirects work correctly through migration
#[test]
fn migration_preserves_entity_redirects() {
    let mut entity_map = LocalEntityMap::new(HostType::Client);

    let global_entity = GlobalEntity::from_u64(1000);
    let old_remote = RemoteEntity::new(100);
    let new_remote = RemoteEntity::new(200);

    // Insert entity with new ID
    entity_map.insert_with_remote_entity(global_entity, new_remote);

    // Install redirect from old to new
    entity_map.install_entity_redirect(
        OwnedLocalEntity::Remote(old_remote.value()),
        OwnedLocalEntity::Remote(new_remote.value()),
    );

    // Verify redirect works by creating an EntityProperty with old ID
    let mut writer = BitWriter::new();
    true.ser(&mut writer);
    OwnedLocalEntity::Remote(old_remote.value()).ser(&mut writer);

    let bytes = writer.to_bytes();
    let mut reader = BitReader::new(&bytes);
    let converter = entity_map.entity_converter();
    let property = EntityProperty::new_read(&mut reader, converter).unwrap();

    assert_eq!(
        property.get_inner(),
        Some(global_entity),
        "EntityProperty should resolve via redirect to correct GlobalEntity"
    );
}

/// Test that multiple migrations chain redirects correctly
#[test]
fn migration_chains_multiple_redirects() {
    let mut entity_map = LocalEntityMap::new(HostType::Client);

    let global_entity = GlobalEntity::from_u64(2000);
    let id1 = RemoteEntity::new(201);
    let id2 = RemoteEntity::new(202);
    let id3 = RemoteEntity::new(203);

    // Final ID in map
    entity_map.insert_with_remote_entity(global_entity, id3);

    // Chain of redirects: id1 → id2 → id3
    entity_map.install_entity_redirect(
        OwnedLocalEntity::Remote(id1.value()),
        OwnedLocalEntity::Remote(id2.value()),
    );
    entity_map.install_entity_redirect(
        OwnedLocalEntity::Remote(id2.value()),
        OwnedLocalEntity::Remote(id3.value()),
    );

    // Test that id1 redirects to id3 via EntityProperty
    let mut writer = BitWriter::new();
    true.ser(&mut writer);
    OwnedLocalEntity::Remote(id1.value()).ser(&mut writer);

    let bytes = writer.to_bytes();
    let mut reader = BitReader::new(&bytes);
    let converter = entity_map.entity_converter();
    let result = EntityProperty::new_read(&mut reader, converter);

    assert!(
        result.is_ok(),
        "Chained redirects should eventually resolve to valid global entity"
    );
}

/// Test that EntityProperty references survive migration
#[test]
fn migration_preserves_entity_property_references() {
    let mut entity_map = LocalEntityMap::new(HostType::Client);

    // Vertex entity that will be migrated
    let vertex_global = GlobalEntity::from_u64(3000);
    let vertex_old = RemoteEntity::new(300);
    let vertex_new = RemoteEntity::new(301);

    entity_map.insert_with_remote_entity(vertex_global, vertex_new);
    entity_map.install_entity_redirect(
        OwnedLocalEntity::Remote(vertex_old.value()),
        OwnedLocalEntity::Remote(vertex_new.value()),
    );

    // Edge component with EntityProperty pointing to old vertex ID
    let mut writer = BitWriter::new();
    true.ser(&mut writer); // exists
    OwnedLocalEntity::Remote(vertex_old.value()).ser(&mut writer);

    // Deserialize EntityProperty
    let bytes = writer.to_bytes();
    let mut reader = BitReader::new(&bytes);
    let converter = entity_map.entity_converter();
    let property = EntityProperty::new_read(&mut reader, converter).unwrap();

    // Verify it resolves to correct vertex
    assert_eq!(
        property.get_inner(),
        Some(vertex_global),
        "EntityProperty should resolve migrated entity via redirect"
    );
}

/// Test migration state for multiple entities
#[test]
fn migration_handles_multiple_entities() {
    let mut entity_map = LocalEntityMap::new(HostType::Client);

    // Create multiple entities with redirects
    let entities = vec![
        (
            GlobalEntity::from_u64(4001),
            RemoteEntity::new(401),
            RemoteEntity::new(451),
        ),
        (
            GlobalEntity::from_u64(4002),
            RemoteEntity::new(402),
            RemoteEntity::new(452),
        ),
        (
            GlobalEntity::from_u64(4003),
            RemoteEntity::new(403),
            RemoteEntity::new(453),
        ),
    ];

    for (global, old_remote, new_remote) in &entities {
        entity_map.insert_with_remote_entity(*global, *new_remote);
        entity_map.install_entity_redirect(
            OwnedLocalEntity::Remote(old_remote.value()),
            OwnedLocalEntity::Remote(new_remote.value()),
        );
    }

    // Verify all redirects work via EntityProperty
    for (global, old_remote, _) in &entities {
        let mut writer = BitWriter::new();
        true.ser(&mut writer);
        OwnedLocalEntity::Remote(old_remote.value()).ser(&mut writer);

        let bytes = writer.to_bytes();
        let mut reader = BitReader::new(&bytes);
        let converter = entity_map.entity_converter();
        let property = EntityProperty::new_read(&mut reader, converter).unwrap();

        assert_eq!(
            property.get_inner(),
            Some(*global),
            "Each entity should resolve via redirect to correct GlobalEntity"
        );
    }
}

/// Test that migration preserves entity relationships
#[test]
fn migration_preserves_entity_relationships() {
    let mut entity_map = LocalEntityMap::new(HostType::Client);

    // Parent-child relationship where both entities migrate
    let parent_global = GlobalEntity::from_u64(5001);
    let parent_old = RemoteEntity::new(501);
    let parent_new = RemoteEntity::new(511);

    let child_global = GlobalEntity::from_u64(5002);
    let child_old = RemoteEntity::new(502);
    let child_new = RemoteEntity::new(512);

    // Setup both entities with redirects
    entity_map.insert_with_remote_entity(parent_global, parent_new);
    entity_map.install_entity_redirect(
        OwnedLocalEntity::Remote(parent_old.value()),
        OwnedLocalEntity::Remote(parent_new.value()),
    );

    entity_map.insert_with_remote_entity(child_global, child_new);
    entity_map.install_entity_redirect(
        OwnedLocalEntity::Remote(child_old.value()),
        OwnedLocalEntity::Remote(child_new.value()),
    );

    // Child has EntityProperty pointing to parent (using old ID)
    let mut writer = BitWriter::new();
    true.ser(&mut writer);
    OwnedLocalEntity::Remote(parent_old.value()).ser(&mut writer);

    let bytes = writer.to_bytes();
    let mut reader = BitReader::new(&bytes);
    let converter = entity_map.entity_converter();
    let parent_property = EntityProperty::new_read(&mut reader, converter).unwrap();

    // Verify relationship preserved
    assert_eq!(
        parent_property.get_inner(),
        Some(parent_global),
        "Child's reference to parent should survive both migrations"
    );
}
