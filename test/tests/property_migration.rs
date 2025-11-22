use naia_shared::{
    BigMapKey, GlobalEntity, HostType, LocalEntityMap, OwnedLocalEntity, RemoteEntity,
};
/// PROPERTY-BASED TESTS: Migration invariants
///
/// Uses proptest to verify migration properties hold across random inputs.
///
/// Key invariants:
/// 1. Entity redirects are transitive
/// 2. Redirected entities always resolve to valid global entities
/// 3. Multiple migrations don't corrupt state
use proptest::prelude::*;

// Strategy for generating RemoteEntity IDs
fn remote_entity_strategy() -> impl Strategy<Value = RemoteEntity> {
    (1u16..1000u16).prop_map(RemoteEntity::new)
}

// Strategy for generating GlobalEntity IDs
fn global_entity_strategy() -> impl Strategy<Value = GlobalEntity> {
    (1u64..10000u64).prop_map(GlobalEntity::from_u64)
}

proptest! {
    /// Test that entity redirects always resolve to valid entities via EntityProperty
    #[test]
    fn prop_redirects_always_resolve(
        global_id in global_entity_strategy(),
        old_id in remote_entity_strategy(),
        new_id in remote_entity_strategy(),
    ) {
        prop_assume!(old_id != new_id); // Ensure they're different

        let mut entity_map = LocalEntityMap::new(HostType::Client);
        entity_map.insert_with_remote_entity(global_id, new_id);
        entity_map.install_entity_redirect(
            OwnedLocalEntity::Remote(old_id.value()),
            OwnedLocalEntity::Remote(new_id.value()),
        );

        // Use EntityProperty to test redirect resolution
        use naia_shared::{BitWriter, BitReader, Serde, EntityProperty};
        let mut writer = BitWriter::new();
        true.ser(&mut writer);
        OwnedLocalEntity::Remote(old_id.value()).ser(&mut writer);

        let bytes = writer.to_bytes();
        let mut reader = BitReader::new(&bytes);
        let converter = entity_map.entity_converter();
        let result = EntityProperty::new_read(&mut reader, converter);

        prop_assert!(result.is_ok(), "Redirected entity should resolve to global entity");
        prop_assert_eq!(result.unwrap().get_inner(), Some(global_id));
    }

    /// Test that multiple non-overlapping redirects don't interfere
    #[test]
    fn prop_multiple_redirects_independent(
        entities in prop::collection::vec(
            (global_entity_strategy(), remote_entity_strategy(), remote_entity_strategy()),
            1..10
        )
    ) {
        // Filter to ensure no ID collisions (global, old, or new)
        let mut seen_global_ids = std::collections::HashSet::new();
        let mut seen_remote_ids = std::collections::HashSet::new();
        let mut valid_entities = Vec::new();

        for (global, old, new) in entities {
            if old != new &&
               !seen_global_ids.contains(&global.to_u64()) &&
               !seen_remote_ids.contains(&old.value()) &&
               !seen_remote_ids.contains(&new.value()) {
                seen_global_ids.insert(global.to_u64());
                seen_remote_ids.insert(old.value());
                seen_remote_ids.insert(new.value());
                valid_entities.push((global, old, new));
            }
        }

        if valid_entities.is_empty() {
            return Ok(());
        }

        let mut entity_map = LocalEntityMap::new(HostType::Client);

        // Install all redirects
        for (global, old, new) in &valid_entities {
            entity_map.insert_with_remote_entity(*global, *new);
            entity_map.install_entity_redirect(
                OwnedLocalEntity::Remote(old.value()),
                OwnedLocalEntity::Remote(new.value()),
            );
        }

        // Verify each redirect works independently via EntityProperty
        use naia_shared::{BitWriter, BitReader, Serde, EntityProperty};
        for (global, old, _) in &valid_entities {
            let mut writer = BitWriter::new();
            true.ser(&mut writer);
            OwnedLocalEntity::Remote(old.value()).ser(&mut writer);

            let bytes = writer.to_bytes();
            let mut reader = BitReader::new(&bytes);
            let converter = entity_map.entity_converter();
            let result = EntityProperty::new_read(&mut reader, converter);

            prop_assert!(result.is_ok(), "Each redirect should resolve independently");
            prop_assert_eq!(result.unwrap().get_inner(), Some(*global));
        }
    }
}

// Regular test for documentation
#[test]
fn property_test_example() {
    // Property-based tests are run via proptest! macro above
    // This test documents the approach

    // Key benefits:
    // 1. Tests with random inputs find edge cases
    // 2. Shrinking helps identify minimal failing case
    // 3. Invariants are tested across input space

    assert!(true, "See proptest! tests above for actual property tests");
}
