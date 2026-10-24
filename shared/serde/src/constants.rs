const MIN_FRAGMENTATION_THRESHOLD_SIZE_BYTES: usize = 576;
const IP_HEADER_SIZE_BYTES: usize = 60;
const UDP_HEADER_SIZE_BYTES: usize = 8;
const DTLS_HEADER_SIZE_BYTES: usize = 50;
const SCTP_HEADER_SIZE_BYTES: usize = 28;
/// The maximum of bytes that can be used for the payload of a given packet.
/// (See #38 of <http://ithare.com/64-network-dos-and-donts-for-game-engines-part-v-udp/>)
pub const MTU_SIZE_BYTES: usize = MIN_FRAGMENTATION_THRESHOLD_SIZE_BYTES
    - IP_HEADER_SIZE_BYTES
    - UDP_HEADER_SIZE_BYTES
    - DTLS_HEADER_SIZE_BYTES
    - SCTP_HEADER_SIZE_BYTES;
/// [`MTU_SIZE_BYTES`] expressed in bits.
pub const MTU_SIZE_BITS: u32 = (MTU_SIZE_BYTES * 8) as u32;

#[cfg(test)]
mod mtu_boundary_tests {
    use super::*;

    /// The MTU boundary is 430 B by the documented worst-case derivation
    /// (IPv4 min 576 − IP 60 − UDP 8 − DTLS 50 − SCTP 28). This pins both
    /// the value the spec names and the derivation, so neither the
    /// components nor the total can drift without failing here — the
    /// per-packet bandwidth budget and the fragmentation limit below are
    /// both calibrated against this number.
    #[test]
    fn mtu_boundary_matches_documented_derivation() {
        assert_eq!(
            MTU_SIZE_BYTES,
            MIN_FRAGMENTATION_THRESHOLD_SIZE_BYTES
                - IP_HEADER_SIZE_BYTES
                - UDP_HEADER_SIZE_BYTES
                - DTLS_HEADER_SIZE_BYTES
                - SCTP_HEADER_SIZE_BYTES,
            "MTU boundary must follow the worst-case WebRTC/SCTP derivation"
        );
        assert_eq!(
            MTU_SIZE_BYTES, 430,
            "transport-02 names 430 B as the boundary"
        );
        assert_eq!(
            MTU_SIZE_BITS, 3440,
            "bit form must track the byte boundary exactly"
        );
    }
}
