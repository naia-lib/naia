//! Bench-instrumentation counters and the (recv, send) connection constructor.
//!
//! Step 4-E.2d (2026-05-16): the `Connection { recv, send }` wrapper struct
//! was dissolved — `RecvState::recv_user_connections` and
//! `SendState::send_user_connections` are now authoritative. Composite
//! call sites (`process_incoming_header` / `write_header` / `read_packet`)
//! split-borrow the two maps directly. This module keeps only:
//!
//! * `bench_send_counters` — fine-grained timing of the send-path
//!   sub-phases, used by `examples/phase4_tick_internals.rs`. Public path
//!   `naia_server::connection::connection::bench_send_counters` is
//!   preserved.
//! * `new_connection_pair` — convenience constructor that builds a
//!   matching `(RecvConnection, SendConnection)` pair and the shared
//!   `Arc<ConnectionShared>` they both clone.

use std::{net::SocketAddr, sync::Arc};

use naia_shared::{BaseConnection, BigMapKey, ChannelKinds, ConnectionConfig, HostType};

use crate::{
    connection::{
        ping_config::PingConfig, recv_connection::RecvConnection, send_connection::SendConnection,
    },
    server::connection_shared::ConnectionShared,
    user::UserKey,
    world::global_world_manager::GlobalWorldManager,
};

/// Fine-grained timing of the send-path sub-phases. Used by
/// `examples/phase4_tick_internals.rs` to localize per-user cost inside
/// the idle send path. Disabled in release unless `bench_instrumentation`.
#[cfg(feature = "bench_instrumentation")]
pub mod bench_send_counters {
    use std::sync::atomic::{AtomicU64, Ordering};
    #[doc(hidden)] pub static NS_COLLECT_MESSAGES: AtomicU64 = AtomicU64::new(0);
    #[doc(hidden)] pub static NS_TAKE_OUTGOING_EVENTS: AtomicU64 = AtomicU64::new(0);
    #[doc(hidden)] pub static NS_SEND_PACKET_LOOP: AtomicU64 = AtomicU64::new(0);
    /// Time spent in `write_packet` (serialization) inside the send loop.
    #[doc(hidden)] pub static NS_WRITE_PACKET: AtomicU64 = AtomicU64::new(0);
    /// Time spent in `io.send_packet` (transport) inside the send loop.
    #[doc(hidden)] pub static NS_IO_SEND: AtomicU64 = AtomicU64::new(0);
    /// Total data packets written across all connections per tick.
    #[doc(hidden)] pub static N_PACKETS_SENT: AtomicU64 = AtomicU64::new(0);
    /// Time spent in `WorldWriter::write_into_packet` (entity/component serialization) per tick.
    #[doc(hidden)] pub static NS_WRITE_UPDATES: AtomicU64 = AtomicU64::new(0);

    /// Resets all counters to zero.
    pub fn reset() {
        NS_COLLECT_MESSAGES.store(0, Ordering::Relaxed);
        NS_TAKE_OUTGOING_EVENTS.store(0, Ordering::Relaxed);
        NS_SEND_PACKET_LOOP.store(0, Ordering::Relaxed);
        NS_WRITE_PACKET.store(0, Ordering::Relaxed);
        NS_IO_SEND.store(0, Ordering::Relaxed);
        N_PACKETS_SENT.store(0, Ordering::Relaxed);
        NS_WRITE_UPDATES.store(0, Ordering::Relaxed);
        naia_shared::bench_take_events_counters::reset();
        crate::server::world_server::bench_iris_counters::reset();
        naia_shared::bench_write_counters::reset();
    }
    /// Returns a snapshot of all counters as a tuple.
    pub fn snapshot() -> (u64, u64, u64) {
        (
            NS_COLLECT_MESSAGES.load(Ordering::Relaxed),
            NS_TAKE_OUTGOING_EVENTS.load(Ordering::Relaxed),
            NS_SEND_PACKET_LOOP.load(Ordering::Relaxed),
        )
    }
    /// Returns `(write_packet_ns, io_send_ns, n_packets_sent, write_updates_ns)` — send-loop sub-breakdown.
    pub fn snapshot_send_breakdown() -> (u64, u64, u64, u64) {
        (
            NS_WRITE_PACKET.load(Ordering::Relaxed),
            NS_IO_SEND.load(Ordering::Relaxed),
            N_PACKETS_SENT.load(Ordering::Relaxed),
            NS_WRITE_UPDATES.load(Ordering::Relaxed),
        )
    }
}

/// Build a matching `(RecvConnection, SendConnection)` pair sharing a
/// single `Arc<ConnectionShared>`. Replaces the old `Connection::new`
/// constructor.
pub fn new_connection_pair(
    connection_config: &ConnectionConfig,
    ping_config: &PingConfig,
    user_address: &SocketAddr,
    user_key: &UserKey,
    channel_kinds: &ChannelKinds,
    global_world_manager: &GlobalWorldManager,
    max_replicated_entities: usize,
) -> (RecvConnection, SendConnection) {
    let (base_recv, base_send) = BaseConnection::new_split(
        connection_config,
        &Some(*user_address),
        HostType::Server,
        user_key.to_u64(),
        channel_kinds,
        global_world_manager,
    );
    let shared = Arc::new(ConnectionShared::new());
    let recv = RecvConnection::new(
        connection_config,
        ping_config,
        *user_address,
        *user_key,
        channel_kinds,
        base_recv,
        Arc::clone(&shared),
    );
    let send = SendConnection::new(
        *user_address,
        *user_key,
        base_send,
        max_replicated_entities,
        shared,
    );
    (recv, send)
}
