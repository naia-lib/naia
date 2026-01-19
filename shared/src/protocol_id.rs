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
