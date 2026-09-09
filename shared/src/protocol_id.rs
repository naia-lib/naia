use naia_serde::SerdeInternal;

/// Name of the HTTP header that carries the client's protocol fingerprint on
/// every auth envelope, and the exact width of its value.
///
/// The fingerprint rides *outside* the credential blob, in its own header, so
/// the server can compare it without decoding anything the client sent as
/// authentication — no base64, no message decode, no user record.
///
/// Defined in `naia-socket-shared`, the one crate beneath every consumer of the
/// name: the client backends that set the header and the session server that
/// reads it sit below `naia-shared` and cannot see anything declared here. It
/// is re-exported rather than restated so there is exactly one place the wire
/// name can change.
pub use naia_socket_shared::{
    stamp_protocol_id_header, PROTOCOL_ID_HEADER, PROTOCOL_ID_HEADER_VALUE_LEN,
    PROTOCOL_MISMATCH_STATUS,
};

/// The socket layer validates the header value's width without being able to
/// name [`ProtocolId`]. If the two widths ever disagree every client is
/// refused, so the disagreement is a compile error instead.
///
/// Written at module scope rather than as an associated constant: an
/// associated constant is only const-evaluated where it is used, so a build
/// that does not reach the use site would not surface the mismatch. This one
/// is evaluated whenever the module is compiled.
const _: () = assert!(ProtocolId::HEX_LEN == PROTOCOL_ID_HEADER_VALUE_LEN);

/// The automatic structural fingerprint of a Protocol configuration.
///
/// Computed by [`Protocol::protocol_id`](crate::Protocol::protocol_id) as a
/// BLAKE3 hash over a canonical, domain-separated preimage covering everything
/// a peer needs to agree on before it can decode: registration order and
/// net-IDs for channels, messages and components; each channel's delivery
/// mode, direction and criticality; resource membership; and the codec grammar
/// version. Compared for equality during the handshake to refuse a peer whose
/// protocol would decode differently.
///
/// # Width
///
/// 128 bits, carried as sixteen bytes in hash order. Two peers with different
/// protocols that nonetheless produce the same fingerprint would connect and
/// misdecode — the one failure mode of this design that fails *open* rather
/// than refusing — so the width is set where neither accidental nor
/// deliberate collision is a live concern.
///
/// The value is stored as `[u8; 16]` rather than a `u128` for two reasons: the
/// wire encoding is then the sixteen hash bytes in order on every host,
/// independent of native endianness, and `naia_serde` has no `u128` scalar
/// impl but does have `[T; N]`.
///
/// # Not a secret
///
/// Anyone holding a released client can compute this value. It is public
/// compatibility metadata and must never be treated as authentication.
#[derive(SerdeInternal, Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct ProtocolId([u8; 16]);

impl ProtocolId {
    /// Number of bytes in the wire representation.
    pub const BYTE_LEN: usize = 16;

    /// Number of characters in the lowercase hex form used on auth envelopes.
    pub const HEX_LEN: usize = Self::BYTE_LEN * 2;

    /// Build an id from the raw sixteen bytes.
    pub const fn from_bytes(bytes: [u8; 16]) -> Self {
        Self(bytes)
    }

    /// The raw sixteen bytes, in wire order.
    pub const fn bytes(&self) -> [u8; 16] {
        self.0
    }

    /// Build an id from a `u128`, little-endian.
    ///
    /// Primarily for tests that need two ids which differ in a controlled way;
    /// real ids come from `Protocol::protocol_id()`.
    pub const fn new(value: u128) -> Self {
        Self(value.to_le_bytes())
    }

    /// The raw value as a `u128`, little-endian.
    pub const fn value(&self) -> u128 {
        u128::from_le_bytes(self.0)
    }

    /// Lowercase hex, exactly [`HEX_LEN`](Self::HEX_LEN) characters, no prefix.
    ///
    /// This is the form carried on auth envelopes: fixed width, so a wrong
    /// length is rejected before the value is even parsed.
    pub fn to_hex(&self) -> String {
        let mut out = String::with_capacity(Self::HEX_LEN);
        for byte in &self.0 {
            out.push_str(&format!("{:02x}", byte));
        }
        out
    }

    /// Parse the hex form produced by [`to_hex`](Self::to_hex).
    ///
    /// Returns `None` for anything that is not exactly
    /// [`HEX_LEN`](Self::HEX_LEN) lowercase-or-uppercase hex digits. Callers
    /// on the auth path must treat `None` exactly as they treat a mismatch —
    /// a lenient "missing" case would let any peer skip the check by omitting
    /// the value.
    pub fn from_hex(text: &str) -> Option<Self> {
        if text.len() != Self::HEX_LEN {
            return None;
        }
        let mut bytes = [0u8; Self::BYTE_LEN];
        for (index, byte) in bytes.iter_mut().enumerate() {
            let start = index * 2;
            *byte = u8::from_str_radix(text.get(start..start + 2)?, 16).ok()?;
        }
        Some(Self(bytes))
    }
}

impl std::fmt::Display for ProtocolId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "ProtocolId({})", self.to_hex())
    }
}

#[cfg(test)]
mod protocol_id_tests {
    use naia_serde::{BitReader, BitWriter, SerdeInternal};

    use super::ProtocolId;

    #[test]
    fn the_raw_value_survives_the_round_trip_through_the_wrapper() {
        assert_eq!(ProtocolId::new(0).value(), 0);
        assert_eq!(ProtocolId::new(u128::MAX).value(), u128::MAX);
        assert_eq!(ProtocolId::new(0xdead_beef).value(), 0xdead_beef);
    }

    #[test]
    fn two_ids_are_equal_exactly_when_their_values_are() {
        assert_eq!(ProtocolId::new(7), ProtocolId::new(7));
        assert_ne!(ProtocolId::new(7), ProtocolId::new(8));
        assert_eq!(ProtocolId::default().value(), 0);
    }

    #[test]
    fn the_display_form_is_a_zero_padded_thirty_two_digit_hex() {
        assert_eq!(
            format!("{}", ProtocolId::new(0xdead_beef)),
            "ProtocolId(efbeadde000000000000000000000000)"
        );
        assert_eq!(
            format!("{}", ProtocolId::new(u128::MAX)),
            "ProtocolId(ffffffffffffffffffffffffffffffff)"
        );
    }

    #[test]
    fn the_wire_form_is_the_sixteen_bytes_in_order() {
        // Endianness of the host must not change what goes on the wire, so the
        // bytes are asserted directly rather than through `value()`.
        let id = ProtocolId::from_bytes([
            0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0a, 0x0b, 0x0c, 0x0d,
            0x0e, 0x0f,
        ]);
        let mut writer = BitWriter::new();
        id.ser(&mut writer);
        let bytes = writer.to_bytes();

        assert_eq!(&bytes[..16], &id.bytes()[..]);
    }

    #[test]
    fn an_id_survives_the_handshake_wire_round_trip() {
        // The handshake compares this value across the connection, so what
        // is read back must be exactly what was written.
        for value in [0u128, 1, 0xdead_beef, u128::MAX] {
            let id = ProtocolId::new(value);
            let mut writer = BitWriter::new();
            id.ser(&mut writer);
            let bytes = writer.to_bytes();
            let mut reader = BitReader::new(&bytes);

            assert_eq!(ProtocolId::de(&mut reader).unwrap(), id);
        }
    }

    #[test]
    fn the_hex_form_round_trips() {
        for value in [0u128, 1, 0xdead_beef, u128::MAX] {
            let id = ProtocolId::new(value);
            let hex = id.to_hex();

            assert_eq!(hex.len(), ProtocolId::HEX_LEN);
            assert_eq!(ProtocolId::from_hex(&hex), Some(id));
        }
    }

    #[test]
    fn a_hex_form_of_the_wrong_shape_is_rejected_rather_than_salvaged() {
        let good = ProtocolId::new(0xdead_beef).to_hex();

        // Every one of these must be indistinguishable from a mismatch at the
        // call site, so none of them may parse.
        assert_eq!(ProtocolId::from_hex(""), None);
        assert_eq!(ProtocolId::from_hex(&good[..ProtocolId::HEX_LEN - 1]), None);
        assert_eq!(ProtocolId::from_hex(&format!("{}0", good)), None);
        assert_eq!(ProtocolId::from_hex(&format!("0x{}", &good[2..])), None);
        assert_eq!(
            ProtocolId::from_hex(&format!("zz{}", &good[2..])),
            None,
            "non-hex digits must not parse"
        );
    }

    #[test]
    fn uppercase_hex_parses_to_the_same_id() {
        let id = ProtocolId::new(0xdead_beef);

        assert_eq!(ProtocolId::from_hex(&id.to_hex().to_uppercase()), Some(id));
    }
}
