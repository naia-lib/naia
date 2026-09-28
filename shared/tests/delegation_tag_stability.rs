//! Card 27945 pin: entity wire tags must not renumber when the
//! `entity_delegation` feature is off.
//!
//! `EntityMessageType` uses the `SerdeInternal` enum encoding: declaration-order
//! ordinal in `ceil(log2(variant_count))` bits, LE-first bit order (`BitWriter`
//! flushes the scratch register with `to_le_bytes`). With 15 variants that is
//! 4 bits, so a lone variant serializes to exactly one byte whose low 4 bits
//! are the ordinal. Any reorder/insert/delete/renumber changes these bytes.
//!
//! This test runs in BOTH feature states (default and `--no-default-features`)
//! and asserts the identical table, so a renumber in either state fails.

use naia_shared::{BitReader, BitWriter, EntityMessageType, Serde};

fn ser_byte(variant: &EntityMessageType) -> u8 {
    let mut writer = BitWriter::new();
    variant.ser(&mut writer);
    let bytes = writer.to_bytes();
    assert_eq!(bytes.len(), 1, "{variant:?} must fit in one byte");
    bytes[0]
}

#[test]
fn entity_message_type_tags_are_stable() {
    // (variant, expected wire byte). Ordinals follow declaration order in
    // shared/src/world/entity/entity_message_type.rs.
    let table: &[(EntityMessageType, u8)] = &[
        (EntityMessageType::Spawn, 0x00),
        (EntityMessageType::SpawnWithComponents, 0x01),
        (EntityMessageType::Despawn, 0x02),
        (EntityMessageType::InsertComponent, 0x03),
        (EntityMessageType::RemoveComponent, 0x04),
        (EntityMessageType::Publish, 0x05),
        (EntityMessageType::Unpublish, 0x06),
        (EntityMessageType::EnableDelegation, 0x07),
        (EntityMessageType::DisableDelegation, 0x08),
        (EntityMessageType::SetAuthority, 0x09),
        (EntityMessageType::Noop, 0x0a),
        (EntityMessageType::RequestAuthority, 0x0b),
        (EntityMessageType::ReleaseAuthority, 0x0c),
        (EntityMessageType::EnableDelegationResponse, 0x0d),
        (EntityMessageType::MigrateResponse, 0x0e),
    ];

    for (variant, expected) in table {
        let byte = ser_byte(variant);
        assert_eq!(
            byte, *expected,
            "wire tag moved for {variant:?}: got {byte:#04x}, want {expected:#04x}"
        );
        // Roundtrip: the byte must decode back to the same variant.
        let bytes = [byte];
        let mut reader = BitReader::new(&bytes);
        let decoded = EntityMessageType::de(&mut reader).expect("must decode");
        assert_eq!(&decoded, variant, "roundtrip mismatch for {variant:?}");
    }
}
