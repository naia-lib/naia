//! Topology-free connection-lifecycle primitives: retry pacing + disconnect
//! classification.
//!
//! naia owns transport, the Bevy client plugin, and [`connection_status`], but
//! historically stopped at [`connect`]. The reusable bit it lacked is the
//! *connection lifecycle* — the mechanics of (a) how fast to retry a connect
//! attempt and (b) whether a given disconnect should be retried at all. Those
//! mechanics are generic networking wisdom, independent of any application's
//! deployment topology, so they live here.
//!
//! [`ReconnectPolicy`] owns ONLY the mechanics. The consumer keeps ownership of
//! every *action*: how to actually perform a connect attempt (transport / auth
//! glue), and what "terminal" means for its app (e.g. bounce to a launcher).
//! The policy communicates its decisions through small returned enums
//! ([`ConnectAttempt`], [`DisconnectAction`]) that the consumer matches on.
//!
//! [`connection_status`]: crate::Client::connection_status
//! [`connect`]: crate::Client::connect

use std::time::Duration;

use naia_bevy_shared::Timer;

use crate::DisconnectReason;

/// The pacing decision returned by [`ReconnectPolicy::poll_attempt`].
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ConnectAttempt {
    /// Perform a connect attempt now.
    Fire,
    /// Hold off — the retry interval hasn't elapsed since the last attempt.
    Wait,
}

/// The classification of a disconnect, returned by
/// [`ReconnectPolicy::classify_disconnect`].
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum DisconnectAction {
    /// Transient drop (timeout / network / reject). Safe to auto-reconnect.
    Reconnect,
    /// Terminal: do NOT auto-reconnect. The remote deliberately evicted this
    /// connection (e.g. another connection for the same account took over).
    /// Auto-reconnecting would re-evict the new connection and flap forever;
    /// the consumer should instead surface this to the user / app layer.
    Terminal,
}

/// Connection-lifecycle pacing for a single naia client connection.
///
/// The consumer owns this inside its own resource/struct (exactly as a hand-
/// rolled retry would own a bare [`Timer`]) and calls [`poll_attempt`] each
/// frame while it wants to be connecting. The first attempt of a fresh cycle
/// fires immediately; subsequent retries are paced behind the retry interval so
/// a failed handshake doesn't hammer the server.
///
/// Disconnect classification is the stateless [`classify_disconnect`]
/// associated function — it encodes only the generic "Kicked is terminal, every
/// other reason is transient" policy and carries no state.
///
/// [`poll_attempt`]: ReconnectPolicy::poll_attempt
/// [`classify_disconnect`]: ReconnectPolicy::classify_disconnect
pub struct ReconnectPolicy {
    send_timer: Timer,
    /// Whether the current connect cycle has already fired its immediate
    /// first attempt. Cleared by [`note_connected`](Self::note_connected) so a
    /// reconnect after a transient drop re-arms "fire immediately".
    cycle_started: bool,
}

impl ReconnectPolicy {
    /// Create a policy that paces retry attempts behind `retry_interval`.
    pub fn new(retry_interval: Duration) -> Self {
        Self {
            send_timer: Timer::new(retry_interval),
            cycle_started: false,
        }
    }

    /// Decide whether to perform a connect attempt now.
    ///
    /// The first call of a fresh cycle returns [`ConnectAttempt::Fire`]
    /// immediately (cold starts and tests don't eat the retry pause). After
    /// that, returns `Fire` only once the retry interval has elapsed since the
    /// previous `Fire`, otherwise [`ConnectAttempt::Wait`].
    ///
    /// The caller should perform the actual connect attempt iff it gets `Fire`.
    /// Call [`note_connected`](Self::note_connected) once the handshake lands so
    /// the next cycle re-arms the immediate first attempt.
    pub fn poll_attempt(&mut self) -> ConnectAttempt {
        if !self.cycle_started {
            // Cold start of a (re)connect cycle: fire immediately and start
            // pacing from now.
            self.cycle_started = true;
            self.send_timer.reset();
            return ConnectAttempt::Fire;
        }
        // Retry path: gate by the timer.
        if self.send_timer.ringing() {
            self.send_timer.reset();
            ConnectAttempt::Fire
        } else {
            ConnectAttempt::Wait
        }
    }

    /// Record that the connection handshake completed.
    ///
    /// Re-arms the immediate first attempt so that if the connection later
    /// drops transiently, the next [`poll_attempt`](Self::poll_attempt) cycle
    /// fires immediately rather than being gated by a stale timer.
    pub fn note_connected(&mut self) {
        self.cycle_started = false;
    }

    /// Classify a disconnect into the generic retry policy.
    ///
    /// [`DisconnectReason::Kicked`] is terminal (the remote deliberately evicted
    /// us — auto-reconnect would flap); every other reason is a transient drop
    /// that is safe to reconnect from. Stateless by design.
    pub fn classify_disconnect(reason: DisconnectReason) -> DisconnectAction {
        match reason {
            DisconnectReason::Kicked => DisconnectAction::Terminal,
            _ => DisconnectAction::Reconnect,
        }
    }
}

#[cfg(all(test, feature = "test_time"))]
mod tests {
    use std::time::Duration;

    use naia_bevy_shared::TestClock;

    use crate::DisconnectReason;

    use super::{ConnectAttempt, DisconnectAction, ReconnectPolicy};

    const INTERVAL_MS: u64 = 5000;

    fn policy() -> ReconnectPolicy {
        ReconnectPolicy::new(Duration::from_millis(INTERVAL_MS))
    }

    #[test]
    fn first_attempt_fires_immediately_then_paces_retries() {
        // Each test runs on its own thread so the thread-local TestClock is
        // clean (mirrors naia's existing test pattern).
        std::thread::spawn(|| {
            TestClock::init(0);
            let mut p = policy();

            // Cold start: fire immediately, no wall-clock wait.
            assert_eq!(p.poll_attempt(), ConnectAttempt::Fire);

            // Immediately after: the retry timer gates us.
            assert_eq!(p.poll_attempt(), ConnectAttempt::Wait);

            // Still gated just before the interval elapses.
            TestClock::advance(INTERVAL_MS - 1);
            assert_eq!(p.poll_attempt(), ConnectAttempt::Wait);

            // Once the interval elapses, the retry fires...
            TestClock::advance(2);
            assert_eq!(p.poll_attempt(), ConnectAttempt::Fire);

            // ...and the timer is reset, so the next attempt waits again.
            assert_eq!(p.poll_attempt(), ConnectAttempt::Wait);
        })
        .join()
        .unwrap();
    }

    #[test]
    fn note_connected_rearms_immediate_first_attempt() {
        std::thread::spawn(|| {
            TestClock::init(0);
            let mut p = policy();

            // Run a full cycle: fire, then get gated.
            assert_eq!(p.poll_attempt(), ConnectAttempt::Fire);
            assert_eq!(p.poll_attempt(), ConnectAttempt::Wait);

            // Handshake lands. A later transient drop must re-arm immediate
            // fire — NOT be gated by the stale timer from the previous cycle.
            p.note_connected();
            assert_eq!(p.poll_attempt(), ConnectAttempt::Fire);
            assert_eq!(p.poll_attempt(), ConnectAttempt::Wait);
        })
        .join()
        .unwrap();
    }

    #[test]
    fn classify_disconnect_kicked_is_terminal_else_reconnect() {
        assert_eq!(
            ReconnectPolicy::classify_disconnect(DisconnectReason::Kicked),
            DisconnectAction::Terminal
        );
        assert_eq!(
            ReconnectPolicy::classify_disconnect(DisconnectReason::TimedOut),
            DisconnectAction::Reconnect
        );
        assert_eq!(
            ReconnectPolicy::classify_disconnect(DisconnectReason::ClientDisconnected),
            DisconnectAction::Reconnect
        );
        assert_eq!(
            ReconnectPolicy::classify_disconnect(DisconnectReason::AuthTimeout),
            DisconnectAction::Reconnect
        );
    }
}
