//! Disconnect-reason wire pins: the consumer reconnect path (`ReconnectPolicy`,
//! `resolve_disconnect_reason`) decides flap-vs-retry from the reason the
//! server put on the wire, so the encoding must be pinned at both layers.
//!
//! `DisconnectReason` uses the `SerdeInternal` enum encoding:
//! declaration-order ordinal in `ceil(log2(variant_count))` bits, LE-first
//! bit order. With 4 variants that is 2 bits, so a lone reason serializes
//! to one byte equal to its ordinal. Any reorder/insert/delete/renumber
//! changes these bytes AND silently reclassifies disconnects (a renumbered
//! `Kicked` arriving as `TimedOut` turns a terminal eviction into an
//! auto-reconnect loop).
//!
//! `HandshakeHeader::ServerDisconnect` (ordinal 10 of 11 variants, 4-bit
//! tag) is the exact packet the server's `write_disconnect` emits and the
//! client's established-state parser consumes. Its 6 bits (4 tag + 2
//! reason) fit in one byte: `0x0A | (reason << 4)`.

use naia_shared::{handshake::HandshakeHeader, BitReader, BitWriter, DisconnectReason, Serde};

fn ser_byte<T: Serde + std::fmt::Debug>(value: &T) -> u8 {
    let mut writer = BitWriter::new();
    value.ser(&mut writer);
    let bytes = writer.to_bytes();
    assert_eq!(bytes.len(), 1, "{value:?} must fit in one byte");
    bytes[0]
}

#[test]
fn disconnect_reason_tags_are_stable() {
    // (variant, expected wire byte). Ordinals follow declaration order in
    // shared/src/types.rs.
    let table: &[(DisconnectReason, u8)] = &[
        (DisconnectReason::ClientDisconnected, 0x00),
        (DisconnectReason::TimedOut, 0x01),
        (DisconnectReason::Kicked, 0x02),
        (DisconnectReason::AuthTimeout, 0x03),
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
        let decoded = DisconnectReason::de(&mut reader).expect("must decode");
        assert_eq!(&decoded, variant, "roundtrip mismatch for {variant:?}");
    }
}

#[test]
fn server_disconnect_header_round_trips_every_reason() {
    // The full server->client packet shape: tag + reason + optional payload
    // (payload is covered by the client's established-state parser; the
    // header+reason pair is what selects reconnect-vs-terminal).
    let table: &[(DisconnectReason, u8)] = &[
        (DisconnectReason::ClientDisconnected, 0x0A),
        (DisconnectReason::TimedOut, 0x1A),
        (DisconnectReason::Kicked, 0x2A),
        (DisconnectReason::AuthTimeout, 0x3A),
    ];

    for (reason, expected) in table {
        let header = HandshakeHeader::ServerDisconnect(*reason);
        assert_eq!(
            ser_byte(&header),
            *expected,
            "wire bytes moved for ServerDisconnect({reason:?})"
        );
        let bytes = [*expected];
        let mut reader = BitReader::new(&bytes);
        let decoded = HandshakeHeader::de(&mut reader).expect("must decode");
        assert_eq!(
            decoded, header,
            "roundtrip mismatch for ServerDisconnect({reason:?})"
        );
    }
}

#[test]
fn forged_header_discriminant_is_an_error() {
    // 11 header variants fill 4 bits; discriminants 11..=15 name no variant.
    // Both parsers (`client.rs` established-state, server `handshaker`) treat
    // a decode failure as a dropped packet, so this must be `Err`, never a
    // panic, even for attacker-controlled bytes.
    for discriminant in [0x0Bu8, 0x0C, 0x0D, 0x0E, 0x0F, 0xFF] {
        let bytes = [discriminant];
        let mut reader = BitReader::new(&bytes);
        assert!(
            HandshakeHeader::de(&mut reader).is_err(),
            "discriminant {discriminant:#04x} must not decode"
        );
    }
}
