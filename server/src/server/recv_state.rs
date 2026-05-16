//! Recv-thread state owned by `WorldServer` (step 4-D).
//!
//! Bundles the recv-thread-exclusive fields that previously lived directly
//! on `WorldServer`. Step 4-E moves this struct out from under
//! `Arc<Mutex<WorldServer>>` into `RecvHandle<E>` directly (and adds
//! `recv_user_connections` + `recv_io` once the symmetric `SendState`
//! extraction lifts the matching pieces out of `WorldServer`).

use std::{collections::HashSet, net::SocketAddr, sync::Arc};

use naia_shared::{DisconnectReason, Timer};

use crate::{
    events::{TickEvents, WorldEvents},
    server::ServerShared,
    user::UserKey,
};

/// Bundles the recv-thread-exclusive `WorldServer` fields (step 4-D).
pub struct RecvState<E: Copy + Eq + std::hash::Hash + Send + Sync> {
    /// Periodic heartbeat send cadence — fires when the recv loop should
    /// transmit a heartbeat packet to live users.
    pub(crate) heartbeat_timer: Timer,
    /// Periodic ping send cadence — fires when the recv loop should send
    /// a ping packet for RTT estimation.
    pub(crate) ping_timer: Timer,
    /// Periodic outer-loop tick that drives `handle_disconnects`.
    pub(crate) timeout_timer: Timer,

    /// Addresses that delivered at least one packet during the current
    /// recv cycle — used to skip the read loop for idle connections.
    pub(crate) addrs_with_new_packets: HashSet<SocketAddr>,
    /// Users flagged for disconnection in this recv cycle (timeout,
    /// manual disconnect, peer disconnect). Drained by the coordinator's
    /// disconnect handler.
    pub(crate) outstanding_disconnects: Vec<(UserKey, DisconnectReason)>,

    /// World events accumulated during this recv cycle. The coordinator
    /// drains via `take_world_events()` before kicking the next sim step.
    pub(crate) incoming_world_events: WorldEvents<E>,
    /// Tick events fired by `recv_server_tick`. Drained by
    /// `take_tick_events()` (which moves into the `ReceiveOutput`
    /// `pending_ticks` list).
    pub(crate) incoming_tick_events: TickEvents,

    /// Shared `Arc<ServerShared<E>>` — gives the recv path lock-free
    /// access to init-only config + the `connection_shared` map (for
    /// observing coordinator-initiated disconnect signals).
    pub(crate) shared: Arc<ServerShared<E>>,
}

impl<E: Copy + Eq + std::hash::Hash + Send + Sync> RecvState<E> {
    /// Construct a new `RecvState` with default timers seeded from the
    /// shared `ServerShared<E>` server-config Arc.
    pub fn new(shared: Arc<ServerShared<E>>) -> Self {
        let heartbeat_interval = shared.server_config.connection.heartbeat_interval;
        let ping_interval = shared.server_config.ping.ping_interval;
        let disconnect_timeout = shared.server_config.connection.disconnection_timeout_duration;
        Self {
            heartbeat_timer: Timer::new(heartbeat_interval),
            ping_timer: Timer::new(ping_interval),
            timeout_timer: Timer::new(disconnect_timeout),
            addrs_with_new_packets: HashSet::new(),
            outstanding_disconnects: Vec::new(),
            incoming_world_events: WorldEvents::new(),
            incoming_tick_events: TickEvents::new(),
            shared,
        }
    }
}
