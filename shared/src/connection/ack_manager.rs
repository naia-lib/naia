use std::collections::HashMap;

use crossbeam_channel::{bounded, Receiver, Sender};

use crate::{types::PacketIndex, wrapping_number::sequence_greater_than};

use super::{
    loss_monitor::LossMonitor, packet_notifiable::PacketNotifiable, packet_type::PacketType,
    sequence_buffer::SequenceBuffer, standard_header::StandardHeader,
};

pub const REDUNDANT_PACKET_ACKS_SIZE: u16 = 32;
const DEFAULT_SEND_PACKETS_SIZE: usize = 256;
// Sized to comfortably hold the worst-case burst of 33 samples per
// `process_incoming_header` call (1 explicit ack + 32 bitfield bits)
// across several queued headers; matches `DEFAULT_SEND_PACKETS_SIZE`.
const ACKED_INDEX_CHANNEL_CAPACITY: usize = DEFAULT_SEND_PACKETS_SIZE;

/// One observation of remote acknowledgement state, pushed by the recv half
/// of the ack pipeline and drained by the send half. `Acked` reflects a bit
/// set in the remote's ack bitfield (or the explicit `sender_ack_index`);
/// `Lost` reflects a bit cleared inside the bitfield window — the remote
/// did not see that packet in time, so we must surface a loss event for it.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum AckSample {
    Acked(PacketIndex),
    Lost(PacketIndex),
}

/// Recv-side half of the ack pipeline (step 4-C.1).
///
/// Owns received-packet bookkeeping (the sliding `received_packets` buffer
/// and `last_recv_packet_index`) and the upstream end of the acked-index
/// channel that the send half drains. After step 4-C.2 lands, this struct
/// lives on `BaseRecvConnection`.
pub struct AckManagerRecv {
    // The last packet index we observed from the remote — echoed back in
    // outbound headers as `sender_ack_index`.
    last_recv_packet_index: PacketIndex,
    // Sliding window of which `sender_packet_index` values we have observed,
    // used to compute the outbound ack bitfield.
    received_packets: SequenceBuffer<ReceivedPacket>,
    // Upstream end of the cross-half handoff channel. Each incoming header
    // pushes one `Acked` per set bit (+ the explicit ack) and one `Lost`
    // per cleared bit in the bitfield window.
    sample_tx: Sender<AckSample>,
}

/// Send-side half of the ack pipeline (step 4-C.1).
///
/// Owns the outbound sequence counter, the `sent_packets` tracker, the
/// loss monitor, the empty-ack one-shot flag, and the downstream end of
/// the acked-index channel pushed by the recv half. After step 4-C.2 lands,
/// this struct lives on `BaseSendConnection`.
///
/// `loss_monitor` placement note (deviates from the §7 field-audit table):
/// the audit lists `loss_monitor` on the recv half with option (a)
/// (recv-only loss accounting). Doing so in 4-C.1 would alter behavior —
/// recv has no `sent_packets` to filter Data vs Heartbeat samples, and the
/// startup window (before either side has sent enough packets to span the
/// 32-bit ack window) would synthesise a stream of "lost" samples for
/// packets that were never sent. The transitional facade therefore keeps
/// `loss_monitor` on the send half, where `sent_packets[idx].packet_type`
/// is available to gate accounting — matching pre-split behavior exactly.
/// The recv-side relocation + option (a) simplification can be revisited
/// in step 4-C.3 once the threaded split is in place.
///
/// `should_send_empty_ack` placement note: the §7 audit calls for deletion
/// (replaced by `ConnectionShared::should_send_empty_ack` atomic). That
/// rewiring touches all `BaseConnection::{mark,take,should}_send_empty_ack`
/// call sites in `naia-server` (and the client-side equivalents) and
/// requires a per-`Connection` `Arc<ConnectionShared>` to be in scope —
/// which only happens once step 4-C.3 plumbs `Arc<ConnectionShared>` into
/// the new recv/send connection wrappers. Until then the flag lives on
/// `AckManagerSend` as a plain `bool`; the facade preserves the existing
/// `BaseConnection` API.
pub struct AckManagerSend {
    // The next index to attach to an outbound packet.
    next_packet_index: PacketIndex,
    // Outstanding packets we sent that have not yet been resolved (acked
    // or lost via the bitfield window). Removed by `drain_samples`.
    sent_packets: HashMap<PacketIndex, SentPacket>,
    // Edge-triggered "an ACK-only packet should go out next chance" flag.
    should_send_empty_ack: bool,
    // Rolling Data-packet loss estimator (the recv->send hop preserves the
    // pre-split semantics; see struct-level note above).
    loss_monitor: LossMonitor,
    // Downstream end of the cross-half handoff channel.
    sample_rx: Receiver<AckSample>,
}

/// Transitional facade owned by `BaseConnection` until step 4-C.2 splits
/// `BaseConnection` itself into `BaseRecvConnection` + `BaseSendConnection`.
///
/// In this transitional state both halves live on the same thread; the
/// crossbeam channel between them is drained synchronously at the end of
/// `process_incoming_header`. Behavior is identical to the pre-split
/// `AckManager` — the only observable difference is the channel hop, which
/// is exercised by the round-trip unit test added in this commit.
pub struct AckManager {
    recv: AckManagerRecv,
    send: AckManagerSend,
}

impl Default for AckManager {
    fn default() -> Self {
        Self::new()
    }
}

impl AckManager {
    /// Creates a new `AckManager`, wiring the two halves together with a
    /// bounded crossbeam channel.
    pub fn new() -> Self {
        let (tx, rx) = bounded(ACKED_INDEX_CHANNEL_CAPACITY);
        Self {
            recv: AckManagerRecv {
                last_recv_packet_index: u16::MAX,
                received_packets: SequenceBuffer::with_capacity(REDUNDANT_PACKET_ACKS_SIZE + 1),
                sample_tx: tx,
            },
            send: AckManagerSend {
                next_packet_index: 0,
                sent_packets: HashMap::with_capacity(DEFAULT_SEND_PACKETS_SIZE),
                should_send_empty_ack: false,
                loss_monitor: LossMonitor::new(),
                sample_rx: rx,
            },
        }
    }

    /// Returns the recent packet loss percentage (0.0–1.0) measured by the loss monitor.
    pub fn packet_loss_pct(&self) -> f32 {
        self.send.loss_monitor.packet_loss_pct()
    }

    /// Returns `true` if an empty ack packet should be sent this tick.
    pub fn should_send_empty_ack(&self) -> bool {
        self.send.should_send_empty_ack
    }

    /// Sets the flag requesting that an empty ack packet be sent.
    pub fn mark_should_send_empty_ack(&mut self) {
        self.send.should_send_empty_ack = true;
    }

    /// Clears the empty-ack flag without returning it.
    pub fn clear_should_send_empty_ack(&mut self) {
        self.send.should_send_empty_ack = false;
    }

    /// Take the should_send_empty_ack flag (returns and clears it).
    pub fn take_should_send_empty_ack(&mut self) -> bool {
        let result = self.send.should_send_empty_ack;
        self.send.should_send_empty_ack = false;
        result
    }

    /// Get the index of the next outgoing packet.
    pub fn next_sender_packet_index(&self) -> PacketIndex {
        self.send.next_packet_index
    }

    /// Returns the sequence index of the most recently received packet.
    pub fn last_received_packet_index(&self) -> PacketIndex {
        self.recv.last_recv_packet_index
    }

    /// Process an incoming packet header. The recv half handles
    /// received-packet bookkeeping and emits `AckSample`s into the
    /// crossbeam channel; the send half drains them, removes acknowledged
    /// entries from `sent_packets`, and fires loss / delivery notifications.
    ///
    /// In step 4-C.1 the drain runs synchronously immediately after the
    /// recv-side work, preserving exact pre-split behavior. Step 4-C.2
    /// will move `drain_samples` to a dedicated entry point on the send
    /// half so the send thread can pull on its own cadence.
    pub fn process_incoming_header(
        &mut self,
        header: &StandardHeader,
        base_packet_notifiables: &mut [&mut dyn PacketNotifiable],
        packet_notifiables: &mut [&mut dyn PacketNotifiable],
    ) {
        self.recv.process_incoming_header(header);
        self.send
            .drain_samples(base_packet_notifiables, packet_notifiables);
    }

    /// Builds and returns the standard header for the next outgoing packet,
    /// advancing the sequence counter.
    pub fn next_outgoing_packet_header(&mut self, packet_type: PacketType) -> StandardHeader {
        let last_rx = self.recv.last_recv_packet_index;
        let ack_bits = self.recv.ack_bitfield();
        self.send
            .next_outgoing_packet_header(packet_type, last_rx, ack_bits)
    }

    /// Splits a freshly-constructed `AckManager` into its two halves wired by
    /// a shared crossbeam channel. Used by `BaseConnection::new()` after the
    /// 4-C.2 split — both halves live on different sub-structs (and, after
    /// step 4-C.3 lands, different threads).
    pub fn new_split() -> (AckManagerRecv, AckManagerSend) {
        let mgr = Self::new();
        (mgr.recv, mgr.send)
    }
}

impl AckManagerRecv {
    /// Returns the sequence index of the most recently received packet.
    pub fn last_received_packet_index(&self) -> PacketIndex {
        self.last_recv_packet_index
    }

    /// Computes the 32-bit ack bitfield for outbound headers, based on
    /// which of the 32 packets preceding `last_recv_packet_index` are
    /// present in `received_packets`.
    pub fn ack_bitfield(&self) -> u32 {
        let last_received_remote_packet_index = self.last_recv_packet_index;
        let mut ack_bitfield: u32 = 0;
        let mut mask: u32 = 1;

        for i in 1..=REDUNDANT_PACKET_ACKS_SIZE {
            let received_packet_index = last_received_remote_packet_index.wrapping_sub(i);
            if self.received_packets.exists(received_packet_index) {
                ack_bitfield |= mask;
            }
            mask <<= 1;
        }

        ack_bitfield
    }

    /// Process an incoming packet header — record receipt and emit
    /// `AckSample`s onto the cross-half channel.
    pub fn process_incoming_header(&mut self, header: &StandardHeader) {
        let sender_packet_index = header.sender_packet_index;
        let sender_ack_index = header.sender_ack_index;
        let mut sender_ack_bitfield = header.sender_ack_bitfield;

        self.received_packets
            .insert(sender_packet_index, ReceivedPacket {});

        // Ensure that `last_recv_packet_index` is monotonically increasing
        // (with wrapping arithmetic).
        if sequence_greater_than(sender_packet_index, self.last_recv_packet_index) {
            self.last_recv_packet_index = sender_packet_index;
        }

        // The explicit `sender_ack_index` is — by definition — acked.
        self.push_sample(AckSample::Acked(sender_ack_index));

        // For each of the 32 prior indices, the bit tells us whether the
        // remote saw that packet. Set => acked; clear => lost (within
        // the bitfield window, the remote definitively didn't see it).
        for i in 1..=REDUNDANT_PACKET_ACKS_SIZE {
            let idx = sender_ack_index.wrapping_sub(i);
            if sender_ack_bitfield & 1 == 1 {
                self.push_sample(AckSample::Acked(idx));
            } else {
                self.push_sample(AckSample::Lost(idx));
            }
            sender_ack_bitfield >>= 1;
        }
    }

    fn push_sample(&self, sample: AckSample) {
        // Best-effort: if the send half is gone or temporarily back-pressured
        // beyond `ACKED_INDEX_CHANNEL_CAPACITY` (33 samples per header), we
        // drop the sample. The worst observable consequence is a missed
        // delivery notification on one packet — the underlying reliable
        // channel will retransmit; loss estimator will recover within the
        // 64-sample window.
        let _ = self.sample_tx.try_send(sample);
    }
}

impl AckManagerSend {
    /// Returns the recent packet loss percentage (0.0–1.0) measured by the loss monitor.
    pub fn packet_loss_pct(&self) -> f32 {
        self.loss_monitor.packet_loss_pct()
    }

    /// Returns `true` if an empty ack packet should be sent this tick.
    pub fn should_send_empty_ack(&self) -> bool {
        self.should_send_empty_ack
    }

    /// Sets the flag requesting that an empty ack packet be sent.
    pub fn mark_should_send_empty_ack(&mut self) {
        self.should_send_empty_ack = true;
    }

    /// Clears the empty-ack flag without returning it.
    pub fn clear_should_send_empty_ack(&mut self) {
        self.should_send_empty_ack = false;
    }

    /// Take the should_send_empty_ack flag (returns and clears it).
    pub fn take_should_send_empty_ack(&mut self) -> bool {
        let result = self.should_send_empty_ack;
        self.should_send_empty_ack = false;
        result
    }

    /// Get the index of the next outgoing packet.
    pub fn next_sender_packet_index(&self) -> PacketIndex {
        self.next_packet_index
    }

    /// Builds the standard header for the next outgoing packet. Takes the
    /// recv-derived ack info (`last_recv_packet_index` + `ack_bitfield`)
    /// from the caller — step 4-C.3 will surface those via
    /// `ConnectionShared` atomics, eliminating the recv-side dependency.
    pub fn next_outgoing_packet_header(
        &mut self,
        packet_type: PacketType,
        last_recv_packet_index: PacketIndex,
        ack_bitfield: u32,
    ) -> StandardHeader {
        let next_packet_index = self.next_packet_index;
        let outgoing = StandardHeader::new(
            packet_type,
            next_packet_index,
            last_recv_packet_index,
            ack_bitfield,
        );
        self.track_packet(packet_type, next_packet_index);
        self.next_packet_index = self.next_packet_index.wrapping_add(1);
        outgoing
    }

    /// Drains pending `AckSample`s pushed by the recv half. For each sample
    /// whose index is still tracked in `sent_packets`, remove it and — if it
    /// was a `Data` packet — update `loss_monitor` and (for `Acked` samples)
    /// fire delivery notifications. Matches pre-split behavior exactly.
    pub fn drain_samples(
        &mut self,
        base_packet_notifiables: &mut [&mut dyn PacketNotifiable],
        packet_notifiables: &mut [&mut dyn PacketNotifiable],
    ) {
        while let Ok(sample) = self.sample_rx.try_recv() {
            match sample {
                AckSample::Acked(idx) => {
                    if let Some(sent_packet) = self.sent_packets.remove(&idx) {
                        if sent_packet.packet_type == PacketType::Data {
                            self.loss_monitor.record_acked();
                            for n in base_packet_notifiables.iter_mut() {
                                n.notify_packet_delivered(idx);
                            }
                            for n in packet_notifiables.iter_mut() {
                                n.notify_packet_delivered(idx);
                            }
                        }
                    }
                }
                AckSample::Lost(idx) => {
                    if let Some(sent_packet) = self.sent_packets.remove(&idx) {
                        if sent_packet.packet_type == PacketType::Data {
                            self.loss_monitor.record_lost();
                        }
                    }
                }
            }
        }
    }

    fn track_packet(&mut self, packet_type: PacketType, packet_index: PacketIndex) {
        self.sent_packets
            .insert(packet_index, SentPacket { packet_type });
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SentPacket {
    pub packet_type: PacketType,
}

#[derive(Clone, Debug, Default)]
pub struct ReceivedPacket;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::connection::packet_notifiable::PacketNotifiable;

    #[derive(Default)]
    struct TestNotifiable {
        delivered: Vec<PacketIndex>,
    }
    impl PacketNotifiable for TestNotifiable {
        fn notify_packet_delivered(&mut self, idx: PacketIndex) {
            self.delivered.push(idx);
        }
    }

    fn header(packet_index: PacketIndex, ack: PacketIndex, bitfield: u32) -> StandardHeader {
        StandardHeader::new(PacketType::Data, packet_index, ack, bitfield)
    }

    /// Round-trip ack through the recv->send channel under packet loss.
    ///
    /// Setup: send three Data packets (indices 0, 1, 2). Remote then
    /// acknowledges index 2 with a bitfield indicating index 1 was lost
    /// (bit 0 clear) and index 0 was received (bit 1 set). After the
    /// recv half processes the header and the send half drains the channel,
    /// we expect:
    ///   * deliveries fired for indices 0 and 2 (no duplicate notification)
    ///   * `sent_packets` cleared for indices 0, 1, 2
    ///   * loss monitor recording 2 acks + 1 loss => loss_pct = 1/3
    #[test]
    fn round_trip_ack_with_loss() {
        let mut mgr = AckManager::new();

        // Send three Data packets.
        let h0 = mgr.next_outgoing_packet_header(PacketType::Data);
        let h1 = mgr.next_outgoing_packet_header(PacketType::Data);
        let h2 = mgr.next_outgoing_packet_header(PacketType::Data);
        assert_eq!(h0.sender_packet_index, 0);
        assert_eq!(h1.sender_packet_index, 1);
        assert_eq!(h2.sender_packet_index, 2);
        assert_eq!(mgr.send.sent_packets.len(), 3);

        // Remote header: explicit ack = 2, bitfield: bit0 (=> idx 1) clear, bit1 (=> idx 0) set.
        let mut notif = TestNotifiable::default();
        mgr.process_incoming_header(&header(99, 2, 0b10), &mut [&mut notif], &mut []);

        // Delivery notifications fire exactly once per Data packet acked.
        let mut delivered = notif.delivered.clone();
        delivered.sort_unstable();
        assert_eq!(delivered, vec![0, 2]);

        // sent_packets is drained for everything in the bitfield window.
        assert!(mgr.send.sent_packets.is_empty());

        // Loss monitor saw 2 acks + 1 loss => 1/3.
        let pct = mgr.packet_loss_pct();
        assert!(
            (pct - (1.0 / 3.0)).abs() < 1e-6,
            "expected loss_pct ≈ 0.333, got {pct}"
        );

        // last_recv_packet_index advanced.
        assert_eq!(mgr.last_received_packet_index(), 99);
    }

    /// Heartbeat packets must not contribute to the loss monitor.
    #[test]
    fn heartbeat_acks_do_not_record_loss() {
        let mut mgr = AckManager::new();
        let _ = mgr.next_outgoing_packet_header(PacketType::Heartbeat);
        let _ = mgr.next_outgoing_packet_header(PacketType::Heartbeat);
        // Remote acks idx 1 with bit 0 clear (idx 0 was "lost").
        let mut notif = TestNotifiable::default();
        mgr.process_incoming_header(&header(50, 1, 0), &mut [&mut notif], &mut []);
        assert!(notif.delivered.is_empty()); // no Data acks -> no notifications
        assert_eq!(mgr.packet_loss_pct(), 0.0); // no Data samples -> 0.0
        assert!(mgr.send.sent_packets.is_empty());
    }

    /// Unknown / wrap-around indices outside the sent set should not panic
    /// and should not corrupt loss accounting.
    #[test]
    fn unknown_indices_are_ignored() {
        let mut mgr = AckManager::new();
        // Don't send anything. Remote acks index 5, bitfield all-set.
        let mut notif = TestNotifiable::default();
        mgr.process_incoming_header(&header(10, 5, u32::MAX), &mut [&mut notif], &mut []);
        assert!(notif.delivered.is_empty());
        assert_eq!(mgr.packet_loss_pct(), 0.0);
    }
}
