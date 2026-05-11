/// Maximum single-fragment payload size in bytes. Messages larger than this
/// are split by `MessageFragmenter` into multiple reliable-channel fragments.
///
/// Derivation (worst-case WebRTC/SCTP path, see `shared/serde/src/constants.rs`):
///
/// ```text
/// IPv4 min MTU            576 B
///  - IP header (max)      - 60 B
///  - UDP header           -  8 B
///  - DTLS overhead (max)  - 50 B
///  - SCTP overhead        - 28 B
///  = packet envelope       430 B   (= MTU_SIZE_BYTES)
///  - naia header budget   - 30 B
///  = FRAGMENTATION_LIMIT   400 B
/// ```
///
/// The 30-byte naia header budget covers `StandardHeader` + packet-kind tag +
/// entity/component framing for the fragment messages that travel in the same
/// packet. Increasing this constant beyond 400 risks overflowing the WebRTC
/// envelope on poor-MTU paths.
pub const FRAGMENTATION_LIMIT_BYTES: usize = 400;
pub const FRAGMENTATION_LIMIT_BITS: u32 = (FRAGMENTATION_LIMIT_BYTES as u32) * 8;
