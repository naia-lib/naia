//! Full handshake-header wire table: every `HandshakeHeader` variant pinned
//! to its declaration-order ordinal (`SerdeInternal` enum encoding, 4-bit
//! tag in 11 variants, LE-first) with a ser/de round-trip.
//!
//! Sibling files pin the disconnect-flavored variants' exact bytes
//! (`disconnect_reason_wire_stability`, `reject_reason_wire_stability`)
//! and the forged-discriminant error path; this file completes the table
//! so a variant reorder or insert can never silently renumber the
//! handshake flow the DWO reconnect path replays on every attempt.
//! The enum carries no `cfg` on its variants, so the table holds in every
//! feature state.

use naia_shared::{
    handshake::HandshakeHeader, handshake::RejectReason, BitReader, BitWriter, DisconnectReason,
    ProtocolId, Serde,
};

fn ser_bytes(value: &HandshakeHeader) -> Vec<u8> {
    let mut writer = BitWriter::new();
    value.ser(&mut writer);
    writer.to_bytes().into_vec()
}

fn round_trip(header: &HandshakeHeader) {
    let bytes = ser_bytes(header);
    let mut reader = BitReader::new(&bytes);
    let decoded = HandshakeHeader::de(&mut reader).expect("must decode");
    assert_eq!(&decoded, header, "roundtrip mismatch for {header:?}");
}

#[test]
fn unit_header_tags_are_stable() {
    // (variant, expected wire byte). Ordinals follow declaration order in
    // shared/src/handshake/header.rs; unit variants fit in one byte.
    let table: &[(HandshakeHeader, u8)] = &[
        (HandshakeHeader::ServerChallengeResponse, 0x01),
        (HandshakeHeader::ClientValidateRequest, 0x02),
        (HandshakeHeader::ServerValidateResponse, 0x03),
        (HandshakeHeader::ServerIdentifyResponse, 0x05),
        (HandshakeHeader::ClientConnectRequest, 0x06),
        (HandshakeHeader::ServerConnectResponse, 0x07),
        (HandshakeHeader::Disconnect, 0x09),
    ];

    for (variant, expected) in table {
        let bytes = ser_bytes(variant);
        assert_eq!(bytes.len(), 1, "{variant:?} must fit in one byte");
        assert_eq!(
            bytes[0], *expected,
            "wire tag moved for {variant:?}: got {:#04x}, want {expected:#04x}",
            bytes[0],
        );
        round_trip(variant);
    }
}

#[test]
fn payload_headers_carry_their_tag_and_round_trip() {
    // (header, expected tag ordinal): the low 4 bits of the first byte are
    // the variant tag; the payload follows.
    let table: &[(HandshakeHeader, u8)] = &[
        (
            HandshakeHeader::ClientChallengeRequest(ProtocolId::default()),
            0x00,
        ),
        (
            HandshakeHeader::ClientIdentifyRequest(ProtocolId::default()),
            0x04,
        ),
        (
            HandshakeHeader::ServerRejectResponse(RejectReason::Auth),
            0x08,
        ),
        (
            HandshakeHeader::ServerDisconnect(DisconnectReason::Kicked),
            0x0A,
        ),
    ];

    for (header, expected_tag) in table {
        let bytes = ser_bytes(header);
        assert_eq!(
            bytes[0] & 0x0F,
            *expected_tag,
            "wire tag moved for {header:?}: got {:#04x}, want {expected_tag:#04x}",
            bytes[0] & 0x0F,
        );
        round_trip(header);
    }
}
