//! Per-tick typed-message router trait for the Recv SubApp.
//!
//! Cyberlith's Recv SubApp drives one [`TickMessageRouter`]
//! implementation per cell. Each pending tick that the recv loop
//! surfaces, the SubApp calls
//! [`route`](TickMessageRouter::route) once, handing it the accumulated
//! [`TickBufferMessages`] for that tick. The implementation decodes the
//! typed messages it cares about (using the standard
//! [`TickBufferMessages::read::<C, M>`] API) and pushes translated
//! actions onto whatever cyberlith-internal queue it owns (e.g.
//! `PendingActions`).
//!
//! Keeping this as a trait — rather than baking the cyberlith-specific
//! translation into naia — keeps naia's surface protocol-agnostic.
//! Naia knows about channels and messages; it does NOT know about
//! cyberlith's `Action` enum or `PlayerCommands` type.

use naia_shared::Tick;

use crate::connection::tick_buffer_messages::TickBufferMessages;

/// Per-tick typed-message router.
///
/// Implementor responsibilities:
/// - Decode every channel/message pair the consumer cares about via
///   [`TickBufferMessages::read::<C, M>`].
/// - Translate decoded messages into the consumer's action enum and
///   enqueue onto the appropriate destination (e.g. a SubApp resource).
/// - Run cheaply: the router is called inline inside the Recv SubApp's
///   per-tick schedule. It should not block on I/O.
///
/// The trait is `Send + Sync + 'static` so the router can live as a
/// resource in the Recv SubApp's world and be moved across the SubApp's
/// dedicated thread boundary.
pub trait TickMessageRouter: Send + Sync + 'static {
    /// Translate all tick-buffered messages for `tick` (across all users)
    /// into the consumer's action queue.
    fn route(&mut self, tick: Tick, messages: TickBufferMessages);
}
