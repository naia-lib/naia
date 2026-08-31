use naia_serde::SerdeInternal;

/// A unique identifier for a Protocol configuration.
///
/// Computed as a BLAKE3 hash of sorted channel, message, and component names.
/// Used during handshake to detect protocol mismatches between client and server.
#[derive(SerdeInternal, Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct ProtocolId(u64);

impl ProtocolId {
    /// Create a new ProtocolId from a raw u64 value.
    /// This is primarily used for testing protocol mismatch scenarios.
    pub fn new(value: u64) -> Self {
        Self(value)
    }

    /// Get the raw u64 value.
    pub fn value(&self) -> u64 {
        self.0
    }
}

impl std::fmt::Display for ProtocolId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "ProtocolId({:016x})", self.0)
    }
}

#[cfg(test)]
mod protocol_id_tests {
    use naia_serde::{BitReader, BitWriter, SerdeInternal};

    use super::ProtocolId;

    #[test]
    fn the_raw_value_survives_the_round_trip_through_the_wrapper() {
        assert_eq!(ProtocolId::new(0).value(), 0);
        assert_eq!(ProtocolId::new(u64::MAX).value(), u64::MAX);
        assert_eq!(ProtocolId::new(0xdead_beef).value(), 0xdead_beef);
    }

    #[test]
    fn two_ids_are_equal_exactly_when_their_values_are() {
        assert_eq!(ProtocolId::new(7), ProtocolId::new(7));
        assert_ne!(ProtocolId::new(7), ProtocolId::new(8));
        assert_eq!(ProtocolId::default().value(), 0);
    }

    #[test]
    fn the_display_form_is_a_zero_padded_sixteen_digit_hex() {
        assert_eq!(
            format!("{}", ProtocolId::new(0xdead_beef)),
            "ProtocolId(00000000deadbeef)"
        );
        assert_eq!(
            format!("{}", ProtocolId::new(u64::MAX)),
            "ProtocolId(ffffffffffffffff)"
        );
    }

    #[test]
    fn an_id_survives_the_handshake_wire_round_trip() {
        // The handshake compares this value across the connection, so what
        // is read back must be exactly what was written.
        for value in [0u64, 1, 0xdead_beef, u64::MAX] {
            let id = ProtocolId::new(value);
            let mut writer = BitWriter::new();
            id.ser(&mut writer);
            let bytes = writer.to_bytes();
            let mut reader = BitReader::new(&bytes);

            assert_eq!(ProtocolId::de(&mut reader).unwrap(), id);
        }
    }
}
