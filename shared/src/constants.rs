/// Maximum single-fragment payload size in bytes. Messages larger than this
/// are split by MessageFragmenter into multiple reliable-channel fragments.
/// 400 B sits comfortably below the standard Ethernet MTU (1,472 B data) and
/// provides plenty of headroom for headers + entity-component batch packing in
/// the same packet.
pub const FRAGMENTATION_LIMIT_BYTES: usize = 400;
pub const FRAGMENTATION_LIMIT_BITS: u32 = (FRAGMENTATION_LIMIT_BYTES as u32) * 8;
