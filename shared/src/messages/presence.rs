//! LOCAL_GUEST_PLAN: controller-presence heartbeat cadence/carrier/freshness
//! producer primitives (naia-only; before P1 step 6).
//!
//! Tests for the [`super`] producer contract: idle heartbeat emission,
//! OS-event emission, replay rejection, carrier handover, and miss-count
//! freshness. The implementation below is intentionally absent on the first
//! commit of this file (red): these tests name the API the producer must
//! supply.
//!
//! # Producer contract (LOCAL_GUEST_PLAN, before P1 step 6)
//!
//! One active carrier epoch per presence scope. The producer emits one frame
//! per heartbeat tick of the carrying connection — the cadence is that
//! connection's `heartbeat_interval`, never a second timer — plus one frame
//! per OS controller event, all independently of gameplay input. Frames apply
//! strict-greater per epoch starting at seq 1; retransmits are idempotent;
//! a higher epoch is a carrier handover (routing keys may change, lifetimes
//! do not); a lower epoch is retired and rejected. Miss counting is in
//! heartbeat ticks; Social owns controller deadlines and stamps
//! received-observation instants, so no wall clock crosses this boundary and
//! no monotonic clocks from different processes are ever compared.

use std::time::Duration;

use crate::ConnectionConfig;

/// Consecutive heartbeat ticks with no applied presence observation before
/// the presence reads stale.
///
/// Derived, not guessed: at the default 4 s heartbeat cadence this is 12 s,
/// strictly inside half the 30 s transport disconnect window, so Social's
/// debounce sees staleness with headroom before the connection itself is
/// declared dead. Pinned by
/// `staleness_fires_before_transport_disconnect_under_default_config`.
/// A stale reading is an observation for Social's deadline owner, never a
/// deadline itself: this row defines no client transport-loss threshold.
pub const HEARTBEAT_LOSS_MISSES: u32 = 3;

/// One active carrier epoch for a controller-presence scope.
///
/// Minted by the producer; a higher value names the replacement carrier
/// after a handover. Never compared across scopes.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct PresenceEpoch {
    value: u64,
}

impl PresenceEpoch {
    /// Names an epoch observed on the wire or in storage. Only the producer
    /// mints new epochs (via [`PresenceProducer::replace_carrier`]); this
    /// names one the peer already issued, it never allocates.
    pub fn from_wire(value: u64) -> Self {
        Self { value }
    }

    /// Reads the raw epoch value, for diagnostics and routing keys.
    pub fn value(self) -> u64 {
        self.value
    }
}

/// Strictly increasing sequence within one [`PresenceEpoch`].
///
/// The first sequence of every epoch is 1; 0 is never emitted and never
/// applies. Checked: the producer freezes instead of wrapping past
/// `u64::MAX`, so a sequence value always names a unique emission.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct PresenceSeq {
    value: u64,
}

impl PresenceSeq {
    /// The first sequence of an epoch. Every epoch starts here.
    pub fn first() -> Self {
        Self { value: 1 }
    }

    /// Names a sequence observed on the wire or in storage. It names a
    /// peer-issued value, it never allocates.
    pub fn from_wire(value: u64) -> Self {
        Self { value }
    }

    /// Reads the raw sequence value, for diagnostics.
    pub fn value(self) -> u64 {
        self.value
    }
}

/// Controller slot bitmap for one presence observation.
///
/// Bit `i` set means the local controller in slot `i` is observed present.
/// Width is a codec decision (16 local slots); content semantics belong to
/// the host controller API via the consumer, never to the transport.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ControllerBitmap {
    bits: u16,
}

impl ControllerBitmap {
    /// No controller observed present.
    pub fn empty() -> Self {
        Self { bits: 0 }
    }

    /// Builds the bitmap from raw slot bits.
    pub fn from_bits(bits: u16) -> Self {
        Self { bits }
    }

    /// Reads the raw slot bits.
    pub fn bits(self) -> u16 {
        self.bits
    }
}

/// One controller-presence observation on the heartbeat carrier.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PresenceFrame {
    /// The carrier epoch that issued this frame.
    pub epoch: PresenceEpoch,
    /// Strictly increasing within [`PresenceFrame::epoch`], from 1.
    pub seq: PresenceSeq,
    /// Observed controller slots at emission.
    pub bitmap: ControllerBitmap,
}

/// Outcome of offering a [`PresenceFrame`] to a [`PresenceTracker`].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PresenceApply {
    /// First frame of a new scope, or a strict-greater sequence on the
    /// active epoch. Resets the miss run.
    Applied,
    /// Exact retransmit of the last applied frame: same observation, no
    /// double effect, miss run untouched.
    Duplicate,
    /// A higher epoch arrived: the carrier handed over. The new epoch
    /// becomes active with this frame as its baseline, and the previous
    /// epoch is retired. Resets the miss run.
    Handover,
    /// Sequence 0, which no epoch ever issues.
    RejectedSeqZero,
    /// Sequence at or below the baseline on the active epoch.
    RejectedStaleSequence,
    /// Epoch below the active one: the carrier it names is retired.
    RejectedRetiredEpoch,
}

/// Emits [`PresenceFrame`]s for one presence scope.
///
/// Cadence is the carrying connection's heartbeat tick: call
/// [`heartbeat_tick`](PresenceProducer::heartbeat_tick) once per tick —
/// the same tick that answers `should_send_heartbeat` — whether or not
/// gameplay input exists, and
/// [`controller_event`](PresenceProducer::controller_event) on every OS
/// controller plug/unplug, which emits immediately. The producer holds no
/// lifetimes and no deadlines; it numbers emissions.
#[derive(Debug)]
pub struct PresenceProducer {
    epoch: PresenceEpoch,
    next_seq: u64,
    exhausted: bool,
    bitmap: ControllerBitmap,
}

impl PresenceProducer {
    /// Starts a scope on `epoch` with the next emission at seq 1 and an
    /// empty bitmap. The first real bitmap arrives via
    /// [`controller_event`](PresenceProducer::controller_event) or the
    /// consumer's initial observation; idle ticks emit empty until then.
    pub fn new(epoch: PresenceEpoch) -> Self {
        Self {
            epoch,
            next_seq: 1,
            exhausted: false,
            bitmap: ControllerBitmap::empty(),
        }
    }

    /// Emits the current bitmap for this heartbeat tick and advances the
    /// sequence, checked. Returns `None` once the `u64` sequence supply is
    /// spent: the producer freezes rather than wrapping and aliasing a
    /// live sequence, and stays frozen.
    pub fn heartbeat_tick(&mut self) -> Option<PresenceFrame> {
        self.emit()
    }

    /// Records an OS controller observation and emits it immediately on
    /// the event path, independent of the heartbeat tick. Returns `None`
    /// under the same exhaustion freeze as
    /// [`heartbeat_tick`](PresenceProducer::heartbeat_tick); the bitmap
    /// is still recorded so a live producer emits it next tick.
    pub fn controller_event(&mut self, bitmap: ControllerBitmap) -> Option<PresenceFrame> {
        self.bitmap = bitmap;
        self.emit()
    }

    /// Retires the current carrier and starts the next epoch at seq 1.
    /// Routing keys may change; no lifetime is touched — this only
    /// renumbers subsequent emissions. Checked: an epoch at `u64::MAX`
    /// cannot hand over and stays put.
    pub fn replace_carrier(&mut self) -> PresenceEpoch {
        if let Some(next) = self.epoch.value.checked_add(1) {
            self.epoch = PresenceEpoch { value: next };
            self.next_seq = 1;
            self.exhausted = false;
        }
        self.epoch
    }

    fn emit(&mut self) -> Option<PresenceFrame> {
        if self.exhausted {
            return None;
        }
        let seq = self.next_seq;
        match self.next_seq.checked_add(1) {
            Some(next_seq) => self.next_seq = next_seq,
            None => self.exhausted = true,
        }
        Some(PresenceFrame {
            epoch: self.epoch,
            seq: PresenceSeq { value: seq },
            bitmap: self.bitmap,
        })
    }
}

/// Applies [`PresenceFrame`]s with strict-greater semantics and counts
/// heartbeat-tick misses for freshness.
///
/// One tracker per presence scope. It stores no lifetimes and sets no
/// deadlines: [`is_stale`](PresenceTracker::is_stale) reports that
/// [`HEARTBEAT_LOSS_MISSES`] consecutive ticks carried no applied
/// observation, for Social — the deadline owner — to consume.
#[derive(Debug)]
pub struct PresenceTracker {
    active_epoch: Option<PresenceEpoch>,
    last_seq: u64,
    misses: u32,
    last_bitmap: Option<ControllerBitmap>,
}

impl PresenceTracker {
    /// Starts with no active epoch: the first applied frame must carry
    /// seq 1 of whatever epoch arrives first.
    pub fn new() -> Self {
        Self {
            active_epoch: None,
            last_seq: 0,
            misses: 0,
            last_bitmap: None,
        }
    }

    /// Offers one frame. Strict-greater per epoch from seq 1; exact
    /// retransmits are idempotent duplicates; higher epochs hand over;
    /// lower epochs and non-advancing sequences are rejected. Applied
    /// and handover frames reset the miss run; every other outcome
    /// leaves it untouched.
    pub fn apply(&mut self, frame: PresenceFrame) -> PresenceApply {
        if frame.seq.value == 0 {
            return PresenceApply::RejectedSeqZero;
        }
        match self.active_epoch {
            None => {
                if frame.seq.value != 1 {
                    return PresenceApply::RejectedStaleSequence;
                }
                self.adopt(frame);
                PresenceApply::Applied
            }
            Some(active) => {
                use std::cmp::Ordering::*;
                match frame.epoch.value.cmp(&active.value) {
                    Less => PresenceApply::RejectedRetiredEpoch,
                    Greater => {
                        if frame.seq.value != 1 {
                            // A new carrier always starts at seq 1; anything
                            // else cannot be its baseline.
                            return PresenceApply::RejectedStaleSequence;
                        }
                        self.adopt(frame);
                        PresenceApply::Handover
                    }
                    Equal => match frame.seq.value.cmp(&self.last_seq) {
                        Greater => {
                            self.last_seq = frame.seq.value;
                            self.last_bitmap = Some(frame.bitmap);
                            self.misses = 0;
                            PresenceApply::Applied
                        }
                        Equal => PresenceApply::Duplicate,
                        Less => PresenceApply::RejectedStaleSequence,
                    },
                }
            }
        }
    }

    /// Records one heartbeat tick that carried no applied observation.
    /// Returns the consecutive-miss run after this tick.
    pub fn note_missed_tick(&mut self) -> u32 {
        self.misses = self.misses.saturating_add(1);
        self.misses
    }

    /// Consecutive ticks with no applied observation.
    pub fn consecutive_misses(&self) -> u32 {
        self.misses
    }

    /// Whether the miss run reached [`HEARTBEAT_LOSS_MISSES`]. An
    /// observation for the deadline owner, never a deadline.
    pub fn is_stale(&self) -> bool {
        self.misses >= HEARTBEAT_LOSS_MISSES
    }

    /// The bitmap of the last applied frame, if any. Survives carrier
    /// handover: replacement changes routing keys, not the observed
    /// controller lifetimes — until the new carrier's own frames arrive,
    /// the last known observation still reads.
    pub fn last_bitmap(&self) -> Option<ControllerBitmap> {
        self.last_bitmap
    }

    fn adopt(&mut self, frame: PresenceFrame) {
        self.active_epoch = Some(frame.epoch);
        self.last_seq = frame.seq.value;
        self.last_bitmap = Some(frame.bitmap);
        self.misses = 0;
    }

    /// The heartbeat-tick cadence this presence rides, sourced from the
    /// carrying connection's config rather than a second timer. Present
    /// so the owner contract names the exact integration point: emit on
    /// the tick that answers `should_send_heartbeat`.
    pub fn cadence(config: &ConnectionConfig) -> Duration {
        config.heartbeat_interval
    }
}

impl Default for PresenceTracker {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::{
        ControllerBitmap, PresenceApply, PresenceEpoch, PresenceProducer, PresenceTracker,
        HEARTBEAT_LOSS_MISSES,
    };
    use crate::ConnectionConfig;

    fn epoch(value: u64) -> PresenceEpoch {
        PresenceEpoch::from_wire(value)
    }

    #[test]
    fn producer_first_frame_is_seq_1() {
        let mut producer = PresenceProducer::new(epoch(7));
        let frame = producer
            .heartbeat_tick()
            .expect("fresh producer has sequence capacity");
        assert_eq!(frame.epoch, epoch(7));
        assert_eq!(frame.seq.value(), 1);
    }

    #[test]
    fn idle_heartbeat_ticks_bump_seq_without_input() {
        let mut producer = PresenceProducer::new(epoch(1));
        let first = producer.heartbeat_tick().expect("capacity");
        let second = producer.heartbeat_tick().expect("capacity");
        assert_eq!(second.seq.value(), first.seq.value() + 1);
        assert_eq!(second.bitmap, first.bitmap);
    }

    #[test]
    fn os_event_emits_immediately_with_bumped_seq() {
        let mut producer = PresenceProducer::new(epoch(1));
        let _ = producer.heartbeat_tick().expect("capacity");
        let event = producer
            .controller_event(ControllerBitmap::from_bits(0b11))
            .expect("capacity");
        assert_eq!(event.seq.value(), 2);
        assert_eq!(event.bitmap, ControllerBitmap::from_bits(0b11));
    }

    #[test]
    fn carrier_replacement_bumps_epoch_and_resets_seq() {
        let mut producer = PresenceProducer::new(epoch(1));
        let _ = producer.heartbeat_tick().expect("capacity");
        let _ = producer.heartbeat_tick().expect("capacity");
        let new_epoch = producer.replace_carrier();
        assert_eq!(new_epoch, epoch(2));
        let frame = producer.heartbeat_tick().expect("capacity");
        assert_eq!(frame.epoch, epoch(2));
        assert_eq!(frame.seq.value(), 1);
    }

    #[test]
    fn tracker_applies_first_seq_1() {
        let mut producer = PresenceProducer::new(epoch(4));
        let mut tracker = PresenceTracker::new();
        let frame = producer.heartbeat_tick().expect("capacity");
        assert_eq!(tracker.apply(frame), PresenceApply::Applied);
    }

    #[test]
    fn tracker_rejects_seq_zero_even_first() {
        let mut tracker = PresenceTracker::new();
        let frame = super::PresenceFrame {
            epoch: epoch(1),
            seq: super::PresenceSeq::from_wire(0),
            bitmap: ControllerBitmap::empty(),
        };
        assert_eq!(tracker.apply(frame), PresenceApply::RejectedSeqZero);
    }

    #[test]
    fn retransmit_of_applied_seq_is_idempotent_duplicate() {
        let mut producer = PresenceProducer::new(epoch(1));
        let mut tracker = PresenceTracker::new();
        let frame = producer.heartbeat_tick().expect("capacity");
        assert_eq!(tracker.apply(frame), PresenceApply::Applied);
        assert_eq!(tracker.apply(frame), PresenceApply::Duplicate);
        assert_eq!(tracker.consecutive_misses(), 0);
    }

    #[test]
    fn stale_seq_after_advance_is_rejected() {
        let mut producer = PresenceProducer::new(epoch(1));
        let mut tracker = PresenceTracker::new();
        let first = producer.heartbeat_tick().expect("capacity");
        let _ = producer.heartbeat_tick().expect("capacity");
        assert_eq!(tracker.apply(first), PresenceApply::Applied);
        // `first` is now behind the tracker's baseline only after a newer
        // frame applies; apply the newer one, then replay the older.
        let mut producer2 = PresenceProducer::new(epoch(1));
        let newer = {
            let _ = producer2.heartbeat_tick().expect("capacity");
            producer2.heartbeat_tick().expect("capacity")
        };
        assert_eq!(tracker.apply(newer), PresenceApply::Applied);
        assert_eq!(tracker.apply(first), PresenceApply::RejectedStaleSequence);
    }

    #[test]
    fn retired_epoch_is_rejected_despite_high_seq() {
        let mut producer = PresenceProducer::new(epoch(2));
        let mut tracker = PresenceTracker::new();
        let frame = producer.heartbeat_tick().expect("capacity");
        assert_eq!(tracker.apply(frame), PresenceApply::Applied);
        let old_epoch_frame = super::PresenceFrame {
            epoch: epoch(1),
            seq: super::PresenceSeq::from_wire(u64::MAX),
            bitmap: ControllerBitmap::empty(),
        };
        assert_eq!(
            tracker.apply(old_epoch_frame),
            PresenceApply::RejectedRetiredEpoch
        );
    }

    #[test]
    fn handover_adopts_higher_epoch_and_retires_the_old() {
        let mut producer = PresenceProducer::new(epoch(1));
        let mut tracker = PresenceTracker::new();
        let v1 = producer.heartbeat_tick().expect("capacity");
        assert_eq!(tracker.apply(v1), PresenceApply::Applied);
        producer.replace_carrier();
        let v2 = producer.heartbeat_tick().expect("capacity");
        assert_eq!(tracker.apply(v2), PresenceApply::Handover);
        // The old epoch is now retired however high its seq claims to be.
        let late_v1 = super::PresenceFrame {
            epoch: epoch(1),
            seq: super::PresenceSeq::from_wire(9000),
            bitmap: ControllerBitmap::empty(),
        };
        assert_eq!(tracker.apply(late_v1), PresenceApply::RejectedRetiredEpoch);
    }

    #[test]
    fn handover_accepts_seq_1_baseline_of_new_epoch() {
        let mut producer = PresenceProducer::new(epoch(1));
        let mut tracker = PresenceTracker::new();
        for _ in 0..5 {
            let frame = producer.heartbeat_tick().expect("capacity");
            let _ = tracker.apply(frame);
        }
        producer.replace_carrier();
        let first_of_new = producer.heartbeat_tick().expect("capacity");
        assert_eq!(first_of_new.seq.value(), 1);
        assert_eq!(tracker.apply(first_of_new), PresenceApply::Handover);
    }

    #[test]
    fn misses_count_to_stale_and_apply_resets() {
        let mut tracker = PresenceTracker::new();
        for _ in 0..HEARTBEAT_LOSS_MISSES - 1 {
            tracker.note_missed_tick();
            assert!(!tracker.is_stale());
        }
        tracker.note_missed_tick();
        assert!(tracker.is_stale());
        // A fresh observation clears the miss run.
        let mut producer = PresenceProducer::new(epoch(1));
        let frame = producer.heartbeat_tick().expect("capacity");
        assert_eq!(tracker.apply(frame), PresenceApply::Applied);
        assert_eq!(tracker.consecutive_misses(), 0);
        assert!(!tracker.is_stale());
    }

    #[test]
    fn staleness_fires_before_transport_disconnect_under_default_config() {
        let config = ConnectionConfig::default();
        let stale_after = HEARTBEAT_LOSS_MISSES * config.heartbeat_interval;
        assert!(
            stale_after < config.disconnection_timeout_duration,
            "presence must go stale ({:?}) strictly before the transport disconnects ({:?})",
            stale_after,
            config.disconnection_timeout_duration,
        );
    }
}
