use std::sync::atomic::{AtomicBool, AtomicU16, AtomicU32, AtomicU64, Ordering};

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

    /// Coordinator-initiated disconnect signal (B4). The coordinator
    /// (e.g. `UserMut::disconnect()`) sets this atomic; the recv thread
    /// observes it on its next loop iteration and processes the disconnect
    /// in the usual way (sets `RecvConnection.manual_disconnect = true`,
    /// pushes a Disconnect event into `incoming_world_events`).
    ///
    /// Provides a thread-safe handshake without needing the coordinator
    /// to reach into `RecvState`.
    pub should_disconnect: AtomicBool,
    /// Cumulative count of packets handed to the socket for this connection
    /// (data, ack-only, heartbeat, flushed pong/handshake responses).
    /// Post-Connected liveness marker (Usher 42587 fork (a)): the send path
    /// writes, the recv-side timeout sweep reads at the drop decision.
    /// Monotonic; wraps only after 2^64 packets.
    pub outbound_packets_sent: AtomicU64,
}

impl ConnectionShared {
    /// Creates a `ConnectionShared` with all fields zeroed (RTT defaults to `0.0 ms`).
    pub fn new() -> Self {
        Self {
            remote_ack_seq: AtomicU16::new(0),
            remote_ack_bitfield: AtomicU32::new(0),
            should_send_empty_ack: AtomicBool::new(false),
            rtt_avg_ms: AtomicU32::new(0_f32.to_bits()),
            should_disconnect: AtomicBool::new(false),
            outbound_packets_sent: AtomicU64::new(0),
        }
    }

    /// Signals the recv thread to process a coordinator-initiated disconnect (B4).
    pub fn set_should_disconnect(&self) {
        self.should_disconnect.store(true, Ordering::Release);
    }

    /// Returns `true` and clears the flag if a coordinator disconnect is pending.
    pub fn take_should_disconnect(&self) -> bool {
        self.should_disconnect.swap(false, Ordering::AcqRel)
    }

    // --- Writer API (recv path) ---

    /// Stores the remote's latest acknowledged packet sequence number.
    pub fn set_remote_ack_seq(&self, seq: u16) {
        self.remote_ack_seq.store(seq, Ordering::Release);
    }

    /// Stores the remote's ACK bitfield (the 32 packets before `remote_ack_seq`).
    pub fn set_remote_ack_bitfield(&self, bits: u32) {
        self.remote_ack_bitfield.store(bits, Ordering::Release);
    }

    /// Signals the send path to emit an ACK-only packet on the next opportunity.
    pub fn set_should_send_empty_ack(&self) {
        self.should_send_empty_ack.store(true, Ordering::Release);
    }

    /// Updates the average round-trip time estimate (in milliseconds).
    pub fn set_rtt_avg_ms(&self, rtt: f32) {
        self.rtt_avg_ms.store(rtt.to_bits(), Ordering::Release);
    }

    /// Records one packet handed to the socket for this connection.
    pub fn note_outbound_packet(&self) {
        self.outbound_packets_sent.fetch_add(1, Ordering::Relaxed);
    }

    // --- Reader API (send path) ---

    /// Returns the remote's latest acknowledged packet sequence number.
    pub fn remote_ack_seq(&self) -> u16 {
        self.remote_ack_seq.load(Ordering::Acquire)
    }

    /// Returns the remote's ACK bitfield (the 32 packets before `remote_ack_seq`).
    pub fn remote_ack_bitfield(&self) -> u32 {
        self.remote_ack_bitfield.load(Ordering::Acquire)
    }

    /// Returns `true` and clears the flag atomically. One-shot per event.
    pub fn take_should_send_empty_ack(&self) -> bool {
        self.should_send_empty_ack.swap(false, Ordering::AcqRel)
    }

    /// Returns the current average round-trip time estimate (in milliseconds).
    pub fn rtt_avg_ms(&self) -> f32 {
        f32::from_bits(self.rtt_avg_ms.load(Ordering::Acquire))
    }

    /// Returns the cumulative outbound packet count for this connection.
    pub fn outbound_packets_sent(&self) -> u64 {
        self.outbound_packets_sent.load(Ordering::Acquire)
    }
}

impl Default for ConnectionShared {
    fn default() -> Self {
        Self::new()
    }
}

// ---- Post-Connected liveness watch (Usher 42587 fork a, oracle pins) ----
#[cfg(test)]
mod connection_shared_tests {
    use super::*;

    #[test]
    fn outbound_packet_count_starts_at_zero_and_accumulates() {
        let shared = ConnectionShared::new();
        assert_eq!(shared.outbound_packets_sent(), 0);
        shared.note_outbound_packet();
        shared.note_outbound_packet();
        shared.note_outbound_packet();
        assert_eq!(
            shared.outbound_packets_sent(),
            3,
            "every packet handed to the socket must be counted for the timeout-drop marker",
        );
    }
}
