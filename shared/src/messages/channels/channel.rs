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
}

/// Tuning parameters for reliable channel delivery and backpressure.
#[derive(Clone)]
pub struct ReliableSettings {
    /// Multiplier on the current RTT that sets the retransmit timeout.
    pub rtt_resend_factor: f32,
    /// Maximum messages to deliver per tick per connection. `None` = unlimited.
    pub max_messages_per_tick: Option<u16>,
    /// Maximum number of unacknowledged messages buffered per connection on
    /// this channel. When the queue is full, `Server::send_message` /
    /// `Client::send_message` returns
    /// `Err(NaiaServerError::MessageQueueFull)` /
    /// `Err(NaiaClientError::MessageQueueFull)` and the caller must decide
    /// whether to retry or discard. `None` = unlimited (not recommended for
    /// production servers). Default: `Some(1024)`.
    pub max_queue_depth: Option<usize>,
}

impl ReliableSettings {
    /// Returns the default `ReliableSettings` (RTT factor 1.5, unlimited throughput, queue cap 1 024).
    pub const fn default() -> Self {
        Self {
            rtt_resend_factor: 1.5,
            max_messages_per_tick: None,
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
