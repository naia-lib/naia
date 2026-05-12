/// Channel abstraction: settings, modes, directions, and criticality.
pub mod channel;
/// Typed channel-kind registry and lookup.
pub mod channel_kinds;
/// Built-in default channel definitions (DefaultUnreliable, DefaultReliable, etc.).
pub mod default_channels;
/// Inbound channel receiver traits and implementations.
pub mod receivers;
/// Outbound channel sender traits and implementations.
pub mod senders;
