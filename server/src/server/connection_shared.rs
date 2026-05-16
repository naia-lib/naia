use std::sync::atomic::{AtomicBool, AtomicU16, AtomicU32, Ordering};

/// ACK and RTT state that crosses the recv/send boundary.
///
/// The recv path writes all four fields; the send path reads them.
/// SPSC atomics — no mutex required. Both `ConnectionRecv` and
/// `ConnectionSend` hold an `Arc<ConnectionShared>` for the same user.
///
/// Field placement matches the naia field audit (2026-05-15 in MISSION_CAPACITY_UPLIFT.md).
pub struct ConnectionShared {
    /// `last_recv_packet_index` — the highest-sequence packet the remote has
    /// confirmed seeing. Recv writes after `process_incoming_header`;
    /// send reads when writing outbound ACK fields.
    pub remote_ack_seq: AtomicU16,

    /// Pre-computed bitfield of the 32 packets before `remote_ack_seq` that
    /// the remote has also received. Recv writes; send reads to avoid retransmitting
    /// already-acknowledged packets.
    pub remote_ack_bitfield: AtomicU32,

    /// Edge-triggered flag: recv sets when an ACK-only response is warranted
    /// (e.g., received a data packet with nothing to piggyback). Send reads
    /// and clears — the flag is one-shot per event.
    pub should_send_empty_ack: AtomicBool,

    /// Round-trip time in milliseconds, stored as `f32::to_bits`. Recv
    /// updates on each pong receipt; send reads for retransmit-timing
    /// decisions in `collect_messages`.
    pub rtt_avg_ms: AtomicU32,
}

impl ConnectionShared {
    pub fn new() -> Self {
        Self {
            remote_ack_seq: AtomicU16::new(0),
            remote_ack_bitfield: AtomicU32::new(0),
            should_send_empty_ack: AtomicBool::new(false),
            rtt_avg_ms: AtomicU32::new(0_f32.to_bits()),
        }
    }

    // --- Writer API (recv path) ---

    pub fn set_remote_ack_seq(&self, seq: u16) {
        self.remote_ack_seq.store(seq, Ordering::Release);
    }

    pub fn set_remote_ack_bitfield(&self, bits: u32) {
        self.remote_ack_bitfield.store(bits, Ordering::Release);
    }

    pub fn set_should_send_empty_ack(&self) {
        self.should_send_empty_ack.store(true, Ordering::Release);
    }

    pub fn set_rtt_avg_ms(&self, rtt: f32) {
        self.rtt_avg_ms.store(rtt.to_bits(), Ordering::Release);
    }

    // --- Reader API (send path) ---

    pub fn remote_ack_seq(&self) -> u16 {
        self.remote_ack_seq.load(Ordering::Acquire)
    }

    pub fn remote_ack_bitfield(&self) -> u32 {
        self.remote_ack_bitfield.load(Ordering::Acquire)
    }

    /// Returns `true` and clears the flag atomically. One-shot per event.
    pub fn take_should_send_empty_ack(&self) -> bool {
        self.should_send_empty_ack.swap(false, Ordering::AcqRel)
    }

    pub fn rtt_avg_ms(&self) -> f32 {
        f32::from_bits(self.rtt_avg_ms.load(Ordering::Acquire))
    }
}

impl Default for ConnectionShared {
    fn default() -> Self {
        Self::new()
    }
}
