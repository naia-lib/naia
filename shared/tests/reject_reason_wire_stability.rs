//! Reject-reason wire pins: the connect path cyberlith consumes decides its
//! next action from the reject reason (`ProtocolMismatch` means fix the
//! build, `Auth` means fix the credentials). The client handshaker test
//! pins the in-memory header handling; this pins the wire encoding, so a
//! variant reorder can never silently swap the two on the wire.
//!
//! `RejectReason` uses the `SerdeInternal` enum encoding:
//! declaration-order ordinal in `ceil(log2(variant_count))` bits, LE-first
//! bit order. With 2 variants that is 1 bit, so a lone reason serializes
//! to one byte equal to its ordinal.
//!
//! `HandshakeHeader::ServerRejectResponse` (ordinal 8 of 11 variants, 4-bit
//! tag) is the exact packet the server's `write_reject_response` emits and
//! the client's mid-handshake parser consumes. Its 5 bits (4 tag + 1
//! reason) fit in one byte: `0x08 | (reason << 4)`.

use naia_shared::{
    handshake::HandshakeHeader, handshake::RejectReason, BitReader, BitWriter, Serde,
};

fn ser_byte<T: Serde + std::fmt::Debug>(value: &T) -> u8 {
    let mut writer = BitWriter::new();
    value.ser(&mut writer);
    let bytes = writer.to_bytes();
    assert_eq!(bytes.len(), 1, "{value:?} must fit in one byte");
    bytes[0]
}

#[test]
fn reject_reason_tags_are_stable() {
    // (variant, expected wire byte). Ordinals follow declaration order in
    // shared/src/handshake/reject_reason.rs.
    let table: &[(RejectReason, u8)] = &[
        (RejectReason::ProtocolMismatch, 0x00),
        (RejectReason::Auth, 0x01),
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
        let decoded = RejectReason::de(&mut reader).expect("must decode");
        assert_eq!(&decoded, variant, "roundtrip mismatch for {variant:?}");
    }
}

#[test]
fn server_reject_response_round_trips_every_reason() {
    let table: &[(RejectReason, u8)] = &[
        (RejectReason::ProtocolMismatch, 0x08),
        (RejectReason::Auth, 0x18),
    ];

    for (reason, expected) in table {
        let header = HandshakeHeader::ServerRejectResponse(*reason);
        assert_eq!(
            ser_byte(&header),
            *expected,
            "wire bytes moved for ServerRejectResponse({reason:?})"
        );
        let bytes = [*expected];
        let mut reader = BitReader::new(&bytes);
        let decoded = HandshakeHeader::de(&mut reader).expect("must decode");
        assert_eq!(
            decoded, header,
            "roundtrip mismatch for ServerRejectResponse({reason:?})"
        );
    }
}
