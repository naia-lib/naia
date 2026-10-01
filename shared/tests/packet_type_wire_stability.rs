//! `PacketType` wire pins: the hand-rolled bespoke encoding (a `Data`
//! fast-path bit plus a 2-bit index for the rest) classifies every packet
//! on the wire. A swapped match arm would silently cross keep-alive with
//! RTT traffic, so all five variants are pinned to exact bytes with a
//! round-trip each.
//!
//! Layout (LE-first): `Data` is a lone `1` bit; anything else is a `0` bit
//! followed by the 2-bit index, so one byte carries the whole encoding:
//! Data 0x01, Heartbeat 0x00, Handshake 0x02, Ping 0x04, Pong 0x06.

use naia_shared::{BitReader, BitWriter, PacketType, Serde};

#[test]
fn packet_type_tags_are_stable() {
    let table: &[(PacketType, u8)] = &[
        (PacketType::Data, 0x01),
        (PacketType::Heartbeat, 0x00),
        (PacketType::Handshake, 0x02),
        (PacketType::Ping, 0x04),
        (PacketType::Pong, 0x06),
    ];

    for (variant, expected) in table {
        let mut writer = BitWriter::new();
        variant.ser(&mut writer);
        let bytes = writer.to_bytes();
        assert_eq!(bytes.len(), 1, "{variant:?} must fit in one byte");
        assert_eq!(
            bytes[0], *expected,
            "wire tag moved for {variant:?}: got {:#04x}, want {expected:#04x}",
            bytes[0],
        );
        let mut reader = BitReader::new(&bytes);
        let decoded = PacketType::de(&mut reader).expect("must decode");
        assert_eq!(&decoded, variant, "roundtrip mismatch for {variant:?}");
    }
}
