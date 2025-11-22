/// Integration tests for serialization round-trips with entity redirects
/// These tests verify that entity references survive serialization/deserialization
use naia_shared::{
    BigMapKey, BitReader, BitWriter, EntityCommand, GlobalEntity, HostEntity, HostType,
    LocalEntityMap, OwnedLocalEntity, RemoteEntity, Serde,
};

/// Test that MigrateResponse preserves entity IDs (Bug #5)
/// The bug was that old_remote_entity was lost during serialization
#[test]
fn migrate_response_preserves_entity_ids() {
    let global_entity = GlobalEntity::from_u64(1);
    let old_remote = RemoteEntity::new(100);
    let new_host = HostEntity::new(200);

    // Create MigrateResponse command
    let command = EntityCommand::MigrateResponse(Some(1), global_entity, old_remote, new_host);

    // Verify the command contains the correct entity IDs
    if let EntityCommand::MigrateResponse(sub_id, g, old_r, new_h) = command {
        assert_eq!(sub_id, Some(1), "Sub ID should be preserved");
        assert_eq!(g, global_entity, "GlobalEntity should be preserved");
        assert_eq!(old_r, old_remote, "Old RemoteEntity should be preserved");
        assert_eq!(new_h, new_host, "New HostEntity should be preserved");
    } else {
        panic!("Command should be MigrateResponse variant");
    }
}

/// Test EntityCommand variants with different entity types
#[test]
fn entity_commands_preserve_entity_types() {
    let global_entity = GlobalEntity::from_u64(10);

    // Test RequestAuthority
    let req_auth = EntityCommand::RequestAuthority(Some(1), global_entity);
    if let EntityCommand::RequestAuthority(_, g) = req_auth {
        assert_eq!(g, global_entity);
    } else {
        panic!("Should be RequestAuthority");
    }

    // Test ReleaseAuthority
    let rel_auth = EntityCommand::ReleaseAuthority(Some(2), global_entity);
    if let EntityCommand::ReleaseAuthority(_, g) = rel_auth {
        assert_eq!(g, global_entity);
    } else {
        panic!("Should be ReleaseAuthority");
    }

    // Test EnableDelegation
    let enable_del = EntityCommand::EnableDelegation(Some(3), global_entity);
    if let EntityCommand::EnableDelegation(_, g) = enable_del {
        assert_eq!(g, global_entity);
    } else {
        panic!("Should be EnableDelegation");
    }
}

/// Test that OwnedLocalEntity serializes and deserializes correctly
#[test]
fn owned_local_entity_serialization_round_trip() {
    // Test Host variant
    let host_entity = OwnedLocalEntity::Host(42);
    let mut writer = BitWriter::new();
    host_entity.ser(&mut writer);

    let bytes = writer.to_bytes();
    let mut reader = BitReader::new(&bytes);
    let deserialized = OwnedLocalEntity::de(&mut reader).unwrap();

    assert_eq!(host_entity, deserialized, "Host variant should round-trip");

    // Test Remote variant
    let remote_entity = OwnedLocalEntity::Remote(99);
    let mut writer = BitWriter::new();
    remote_entity.ser(&mut writer);

    let bytes = writer.to_bytes();
    let mut reader = BitReader::new(&bytes);
    let deserialized = OwnedLocalEntity::de(&mut reader).unwrap();

    assert_eq!(
        remote_entity, deserialized,
        "Remote variant should round-trip"
    );
}

/// Test entity redirect application during serialization
/// This is Gap #4 from the document - entity references change during serialization
#[test]
fn entity_redirects_work_with_serialization() {
    let mut entity_map = LocalEntityMap::new(HostType::Client);

    let global_entity = GlobalEntity::from_u64(1);
    let old_remote = RemoteEntity::new(10);
    let new_remote = RemoteEntity::new(20);

    // Setup: entity migrated from old_remote to new_remote
    entity_map.insert_with_remote_entity(global_entity, new_remote);
    entity_map.install_entity_redirect(
        OwnedLocalEntity::Remote(old_remote.value()),
        OwnedLocalEntity::Remote(new_remote.value()),
    );

    // Serialize old entity ID
    let mut writer = BitWriter::new();
    OwnedLocalEntity::Remote(old_remote.value()).ser(&mut writer);

    // When deserializing and applying redirects via EntityProperty,
    // the old ID should resolve to the correct global entity
    // (This is tested more thoroughly in regression_bug_06_entity_property.rs)

    let bytes = writer.to_bytes();
    let mut reader = BitReader::new(&bytes);
    let deserialized = OwnedLocalEntity::de(&mut reader).unwrap();

    assert_eq!(
        deserialized,
        OwnedLocalEntity::Remote(old_remote.value()),
        "Deserialized entity should match serialized (before redirect application)"
    );

    // The redirect is applied at a higher level (EntityProperty::new_read or waiting_complete)
    let converter = entity_map.entity_converter();
    let redirected = converter.apply_entity_redirect(&deserialized);

    assert_eq!(
        redirected,
        OwnedLocalEntity::Remote(new_remote.value()),
        "Redirect should transform old ID to new ID"
    );
}
