use crate::named::Named;

/// Marker trait for types that represent a named communication channel.
pub trait Channel: Named + 'static {}

/// Configuration for a channel: delivery mode, traffic direction, and priority criticality.
#[derive(Clone)]
pub struct ChannelSettings {
    /// Delivery semantics (ordered/unordered, reliable/unreliable, or tick-buffered).
    pub mode: ChannelMode,
    /// Which endpoint(s) may send on this channel.
    pub direction: ChannelDirection,
    /// Priority tier used by the unified priority-sort send loop. Contributes
    /// `base_gain()` per tick of message age to each message's on-the-fly
    /// accumulator. Defaults via `ChannelCriticality::default_for(&mode)`.
    pub criticality: ChannelCriticality,
}

impl ChannelSettings {
    /// Creates a `ChannelSettings` with the given mode and direction, deriving default criticality from the mode.
    pub fn new(mode: ChannelMode, direction: ChannelDirection) -> Self {
        if mode.tick_buffered() && direction != ChannelDirection::ClientToServer {
            panic!("TickBuffered Messages are only allowed to be sent from Client to Server");
        }

        let criticality = ChannelCriticality::default_for(&mode);
        Self {
            mode,
            direction,
            criticality,
        }
    }

    /// Override the channel's priority tier. Builder-style.
    pub fn with_criticality(mut self, criticality: ChannelCriticality) -> Self {
        self.criticality = criticality;
        self
    }

    /// Returns `true` if this channel guarantees delivery (all reliable modes).
    pub fn reliable(&self) -> bool {
        match &self.mode {
            ChannelMode::UnorderedUnreliable => false,
            ChannelMode::SequencedUnreliable => false,
            ChannelMode::UnorderedReliable(_) => true,
            ChannelMode::SequencedReliable(_) => true,
            ChannelMode::OrderedReliable(_) => true,
            ChannelMode::TickBuffered(_) => false,
        }
    }

    /// Returns `true` if this channel uses tick-buffered delivery.
    pub fn tick_buffered(&self) -> bool {
        self.mode.tick_buffered()
    }

    /// Returns `true` if the client may send on this channel.
    pub fn can_send_to_server(&self) -> bool {
        match &self.direction {
            ChannelDirection::ClientToServer => true,
            ChannelDirection::ServerToClient => false,
            ChannelDirection::Bidirectional => true,
        }
    }

    /// Returns `true` if the server may send on this channel.
    pub fn can_send_to_client(&self) -> bool {
        match &self.direction {
            ChannelDirection::ClientToServer => false,
            ChannelDirection::ServerToClient => true,
            ChannelDirection::Bidirectional => true,
        }
    }

    /// Returns `true` if this channel supports bidirectional reliable request/response messaging.
    pub fn can_request_and_respond(&self) -> bool {
        self.reliable() && self.can_send_to_server() && self.can_send_to_client()
    }

    /// Canonical byte encoding of every wire-relevant value in these settings,
    /// for the protocol fingerprint preimage.
    ///
    /// Layout: the mode encoding from [`ChannelMode::schema_bytes`], then the
    /// direction discriminant, then the criticality discriminant. Fixed order,
    /// fixed widths, no separators — the mode encoding is self-delimiting
    /// because each discriminant determines its own payload length, so the two
    /// trailing bytes can never be mistaken for part of it.
    pub fn schema_bytes(&self) -> Vec<u8> {
        let mut out = self.mode.schema_bytes();
        out.push(self.direction.schema_discriminant());
        out.push(self.criticality.schema_discriminant());
        out
    }
}

/// Tuning parameters for reliable channel delivery and backpressure.
#[derive(Clone)]
pub struct ReliableSettings {
    /// Multiplier on the current RTT that sets the retransmit timeout.
    pub rtt_resend_factor: f32,
    /// Maximum number of unacknowledged messages buffered per connection on
    /// this channel. When the queue is full, `Server::send_message` /
    /// `Client::send_message` returns
    /// `Err(NaiaServerError::MessageQueueFull)` /
    /// `Err(NaiaClientError::MessageQueueFull)` and the caller must decide
    /// whether to retry or discard. `None` = unlimited (not recommended for
    /// production servers). Default: `Some(1024)`.
    ///
    /// This value does double duty: it is also the **receive window**. A peer may
    /// not send a message index more than `max_queue_depth` ahead of the oldest
    /// index we are still waiting for, because its own sender cannot have more
    /// than that many messages outstanding. Indices beyond the window are dropped
    /// with a warning, which bounds the per-connection, per-channel memory a
    /// remote peer can pin. Setting `None` disables the send cap *and* the receive
    /// window.
    pub max_queue_depth: Option<usize>,
}

impl ReliableSettings {
    /// Returns the default `ReliableSettings` (RTT factor 1.5, queue cap 1 024).
    pub const fn default() -> Self {
        Self {
            rtt_resend_factor: 1.5,
            max_queue_depth: Some(1024),
        }
    }
}

/// Capacity settings for a tick-buffered channel.
#[derive(Clone)]
pub struct TickBufferSettings {
    /// Describes a maximum of messages that may be kept in the buffer.
    /// Oldest messages are pruned out first.
    pub message_capacity: usize,
}

impl TickBufferSettings {
    /// Returns the default `TickBufferSettings` with a message capacity of 64.
    pub const fn default() -> Self {
        Self {
            message_capacity: 64,
        }
    }
}

/// Delivery semantics for a channel.
#[derive(Clone)]
pub enum ChannelMode {
    /// Messages are delivered at most once with no ordering guarantee.
    UnorderedUnreliable,
    /// Only the latest message per sequence slot is delivered; older ones are silently dropped.
    SequencedUnreliable,
    /// Every message is delivered exactly once; arrival order is not guaranteed.
    UnorderedReliable(ReliableSettings),
    /// Every message is delivered exactly once; only the latest-sequenced message is surfaced.
    SequencedReliable(ReliableSettings),
    /// Every message is delivered exactly once in the original send order.
    OrderedReliable(ReliableSettings),
    /// Messages are held in a fixed-capacity buffer tied to a specific server tick.
    TickBuffered(TickBufferSettings),
}

impl ChannelMode {
    /// Returns `true` if this mode is `TickBuffered`.
    pub fn tick_buffered(&self) -> bool {
        matches!(self, ChannelMode::TickBuffered(_))
    }

    /// Stable identifier for this mode inside the protocol fingerprint preimage.
    ///
    /// Hand-pinned rather than derived from variant position: the fingerprint
    /// is a claim about the wire, so a value here must never move because
    /// somebody reordered the enum for readability. Adding a mode takes the
    /// next free number; an existing number is never reused for a different
    /// mode.
    ///
    /// This identifies the *variant only*. It is never hashed on its own —
    /// see [`schema_bytes`](Self::schema_bytes), which also carries the
    /// variant's payload. A fingerprint built from the discriminant alone
    /// would call two protocols equal when one of them has, say, a different
    /// reliable receive window, which is a value both peers must agree on.
    pub const fn schema_discriminant(&self) -> u8 {
        match self {
            ChannelMode::UnorderedUnreliable => 0,
            ChannelMode::SequencedUnreliable => 1,
            ChannelMode::UnorderedReliable(_) => 2,
            ChannelMode::SequencedReliable(_) => 3,
            ChannelMode::OrderedReliable(_) => 4,
            ChannelMode::TickBuffered(_) => 5,
        }
    }

    /// Canonical byte encoding of this mode *and its full payload*, for the
    /// protocol fingerprint preimage.
    ///
    /// Layout, in order:
    ///
    /// - the [`schema_discriminant`](Self::schema_discriminant) byte;
    /// - for the three reliable modes, `ReliableSettings`:
    ///   `rtt_resend_factor` as its four canonical IEEE-754 bits
    ///   little-endian, then `max_queue_depth` as `0x00` for `None` or `0x01`
    ///   followed by the value as a little-endian `u64`;
    /// - for `TickBuffered`, `message_capacity` as a little-endian `u64`;
    /// - for the two unreliable modes, nothing.
    ///
    /// Every payload is fixed-width for a given discriminant, so the encoding
    /// is self-delimiting and needs no length prefix.
    ///
    /// `usize` is widened to `u64` deliberately: a 32-bit and a 64-bit peer
    /// with identical settings must produce identical bytes, so the native
    /// width must not reach the preimage.
    pub fn schema_bytes(&self) -> Vec<u8> {
        let mut out = vec![self.schema_discriminant()];
        match self {
            ChannelMode::UnorderedUnreliable | ChannelMode::SequencedUnreliable => {}
            ChannelMode::UnorderedReliable(settings)
            | ChannelMode::SequencedReliable(settings)
            | ChannelMode::OrderedReliable(settings) => {
                out.extend_from_slice(&canonical_f32_bits(settings.rtt_resend_factor));
                match settings.max_queue_depth {
                    None => out.push(0),
                    Some(depth) => {
                        out.push(1);
                        out.extend_from_slice(&(depth as u64).to_le_bytes());
                    }
                }
            }
            ChannelMode::TickBuffered(settings) => {
                out.extend_from_slice(&(settings.message_capacity as u64).to_le_bytes());
            }
        }
        out
    }
}

/// The IEEE-754 bits of `value`, little-endian, with every NaN collapsed onto
/// one pattern.
///
/// A protocol whose `rtt_resend_factor` is NaN is already misconfigured, but
/// the fingerprint must still be a function of the configuration rather than
/// of which NaN bit pattern a particular compiler happened to produce —
/// otherwise two peers built from the same source could disagree. `-0.0` is
/// left distinct from `0.0`; they are different configured values.
fn canonical_f32_bits(value: f32) -> [u8; 4] {
    if value.is_nan() {
        f32::NAN.to_bits().to_le_bytes()
    } else {
        value.to_bits().to_le_bytes()
    }
}

/// Permitted send direction(s) for a channel.
#[derive(Clone, Eq, PartialEq)]
pub enum ChannelDirection {
    /// Only the client may send on this channel.
    ClientToServer,
    /// Only the server may send on this channel.
    ServerToClient,
    /// Both endpoints may send on this channel.
    Bidirectional,
}

impl ChannelDirection {
    /// Stable identifier for this direction inside the protocol fingerprint
    /// preimage. Hand-pinned for the same reason as
    /// [`ChannelMode::schema_discriminant`].
    pub const fn schema_discriminant(&self) -> u8 {
        match self {
            ChannelDirection::ClientToServer => 0,
            ChannelDirection::ServerToClient => 1,
            ChannelDirection::Bidirectional => 2,
        }
    }
}

/// Priority tier for a channel in the unified priority-sort send loop.
///
/// Each message's accumulator grows per tick by `base_gain()` × tick-age.
/// Higher criticality → faster accumulator growth → earlier eligibility in the
/// sorted drain. Reliable channels never drop items; criticality only changes
/// when they egress relative to other channels and entity bundles.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ChannelCriticality {
    /// Background traffic (e.g. non-urgent unreliable). `base_gain() = 0.5`.
    Low,
    /// Default tier. `base_gain() = 1.0`.
    Normal,
    /// Control traffic that must head the queue (e.g. auth, connection
    /// lifecycle, critical RPCs). `base_gain() = 10.0`.
    High,
}

impl ChannelCriticality {
    /// Default tier applied by `ChannelSettings::new` based on channel mode.
    /// TickBuffered → High (must land in the right tick window). Everything
    /// else → Normal. Callers can override via `with_criticality()`.
    pub const fn default_for(mode: &ChannelMode) -> Self {
        match mode {
            ChannelMode::TickBuffered(_) => ChannelCriticality::High,
            _ => ChannelCriticality::Normal,
        }
    }

    /// Stable identifier for this tier inside the protocol fingerprint
    /// preimage. Hand-pinned for the same reason as
    /// [`ChannelMode::schema_discriminant`].
    pub const fn schema_discriminant(&self) -> u8 {
        match self {
            ChannelCriticality::Low => 0,
            ChannelCriticality::Normal => 1,
            ChannelCriticality::High => 2,
        }
    }

    /// Per-tick priority gain applied to every queued message on this channel.
    pub const fn base_gain(&self) -> f32 {
        match self {
            ChannelCriticality::Low => 0.5,
            ChannelCriticality::Normal => 1.0,
            ChannelCriticality::High => 10.0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // A-BDD-5: Channel built with with_criticality(Low) on a normally-Normal
    // mode gets Low base_gain in sort.
    #[test]
    fn with_criticality_overrides_mode_default() {
        let s = ChannelSettings::new(
            ChannelMode::UnorderedReliable(ReliableSettings::default()),
            ChannelDirection::Bidirectional,
        );
        assert_eq!(s.criticality, ChannelCriticality::Normal);
        let s2 = s.with_criticality(ChannelCriticality::Low);
        assert_eq!(s2.criticality, ChannelCriticality::Low);
        assert!((s2.criticality.base_gain() - 0.5).abs() < f32::EPSILON);
    }

    #[test]
    fn tick_buffered_defaults_to_high() {
        let s = ChannelSettings::new(
            ChannelMode::TickBuffered(TickBufferSettings::default()),
            ChannelDirection::ClientToServer,
        );
        assert_eq!(s.criticality, ChannelCriticality::High);
        assert!((s.criticality.base_gain() - 10.0).abs() < f32::EPSILON);
    }

    #[test]
    fn unreliable_defaults_to_normal() {
        let s = ChannelSettings::new(
            ChannelMode::UnorderedUnreliable,
            ChannelDirection::Bidirectional,
        );
        assert_eq!(s.criticality, ChannelCriticality::Normal);
    }

    // A-BDD-6 support: base_gain ordering. High > Normal > Low.
    #[test]
    fn base_gain_ordering() {
        let high = ChannelCriticality::High.base_gain();
        let normal = ChannelCriticality::Normal.base_gain();
        let low = ChannelCriticality::Low.base_gain();
        assert!(high > normal);
        assert!(normal > low);
    }
}
