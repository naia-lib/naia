//! Recv-thread state owned by `WorldServer` (step 4-D).
//!
//! Bundles the recv-thread-exclusive fields that previously lived directly
//! on `WorldServer`. Step 4-E moves this struct out from under
//! `Arc<Mutex<WorldServer>>` into `RecvHandle<E>` directly (and adds
//! `recv_user_connections` + `recv_io` once the symmetric `SendState`
//! extraction lifts the matching pieces out of `WorldServer`).

use std::{
    collections::{HashMap, HashSet},
    net::SocketAddr,
    sync::Arc,
};

use log::warn;

use naia_shared::{
    handshake::HandshakeHeader, BitReader, DisconnectReason, OwnedBitReader, PacketType,
    Serde, SerdeErr, StandardHeader, Tick, Timer,
};

use crate::{
    connection::{io::RecvIo, RecvConnection},
    error::NaiaServerError,
    events::{TickEvents, WorldEvents},
    handshake::HandshakeManager,
    server::ServerShared,
    user::UserKey,
};

/// Bundles the recv-thread-exclusive `WorldServer` fields (step 4-D).
pub struct RecvState<E: Copy + Eq + std::hash::Hash + Send + Sync> {
    /// Periodic outer-loop tick that drives `handle_disconnects`.
    pub(crate) timeout_timer: Timer,

    /// Addresses that delivered at least one Data packet during the
    /// current recv cycle — drives `process_all_packets`'s per-address
    /// loop.
    pub(crate) addrs_with_new_packets: HashSet<SocketAddr>,

    /// Addresses that delivered at least ONE packet of ANY type during
    /// the current recv cycle (step 4-F.naia.c.2b). Superset of
    /// `addrs_with_new_packets`. Consumed by `SendState::process_recv_packets`
    /// for the per-address `drain_acks` + `process_received_commands`
    /// sweep that used to run inline at the tail of the recv loop.
    pub(crate) received_addresses: HashSet<SocketAddr>,
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

    /// Per-address map of recv-side connection halves.
    ///
    /// Populated by `WorldServer::into_pipeline_states()` when the
    /// pipeline coordinator takes ownership; empty in serial mode
    /// (where `WorldServer::user_connections` holds the full
    /// `Connection` wrappers). Once the recv thread runs against
    /// `RecvState` directly (step 4-F), this map replaces
    /// `WorldServer::user_connections` for recv-path lookups.
    pub recv_user_connections: HashMap<SocketAddr, RecvConnection>,

    /// Receive half of the transport (step 4-E.2a). Owned here so the
    /// recv thread has exclusive mutable access without locking.
    pub(crate) recv_io: RecvIo,

    /// Data packets buffered by the recv path for later coordinator-side
    /// decode (step 4-E.2e). Each tuple is `(addr, client_tick,
    /// owned_reader)` where the reader's internal state has already
    /// advanced past the `StandardHeader` and the `Tick` field — the
    /// decoder starts at the message/world section directly.
    ///
    /// The recv thread reads `StandardHeader` + the client tick, calls
    /// `RecvConnection::process_incoming_header` (so the ACK snapshot
    /// publishes immediately on the cross-half atomic), then snapshots
    /// the in-flight `BitReader` into an `OwnedBitReader` and pushes
    /// here. In serial mode the queue drains inline at the tail of
    /// `WorldServer::receive_all_packets`; in pipeline mode the
    /// coordinator drains via `SendHandle::process_recv_packets` (4-E.2f).
    pub(crate) pending_data_packets: Vec<(SocketAddr, Tick, OwnedBitReader)>,
}

impl<E: Copy + Eq + std::hash::Hash + Send + Sync> RecvState<E> {
    /// Construct a new `RecvState` with default timers seeded from the
    /// shared `ServerShared<E>` server-config Arc.
    pub fn new(shared: Arc<ServerShared<E>>, recv_io: RecvIo) -> Self {
        let disconnect_timeout = shared.server_config.connection.disconnection_timeout_duration;
        Self {
            timeout_timer: Timer::new(disconnect_timeout),
            addrs_with_new_packets: HashSet::new(),
            received_addresses: HashSet::new(),
            outstanding_disconnects: Vec::new(),
            incoming_world_events: WorldEvents::new(),
            incoming_tick_events: TickEvents::new(),
            shared,
            recv_user_connections: HashMap::new(),
            recv_io,
            pending_data_packets: Vec::new(),
        }
    }
}

impl<E: Copy + Eq + std::hash::Hash + Send + Sync> RecvState<E> {
    /// Awaitable readiness of the recv transport, if event-driven (see
    /// [`crate::transport::PacketReceiver::readiness`]).
    pub fn readiness(&self) -> Option<crate::transport::PacketReadiness> {
        self.recv_io.readiness()
    }

    /// Recv-only socket-read loop (step 4-F.naia.c.2b).
    ///
    /// Body of `WorldServer::receive_all_packets`'s recv-side work,
    /// hoisted onto `RecvState` so `RecvHandle::receive` can drive it
    /// directly on the recv thread. Touches ONLY `self.*` fields
    /// (`recv_user_connections`, `recv_io`, `pending_data_packets`,
    /// `received_addresses`, `addrs_with_new_packets`, the disconnect
    /// queue, `incoming_world_events`) and `self.shared` (for the
    /// outbound-packet / handshake ferry queues + `time_manager` read).
    /// Specifically does NOT touch `SendState` — every cross-half hop
    /// goes through a queue on `ServerShared`.
    ///
    /// In serial mode `WorldServer::receive_all_packets` calls this and
    /// then runs the coordination-stage `drain_pending_handshakes` +
    /// `SendState::process_recv_packets` post-passes; in pipeline mode
    /// `RecvHandle::receive` calls this and drains the
    /// `received_addresses` + `pending_data_packets` fields into the
    /// returned `ReceiveOutput` for the coordinator to feed into
    /// `SendHandle::process_recv_packets`.
    pub fn receive(&mut self) {
        // Tick the recv-side bandwidth monitor.
        self.recv_io.tick_bandwidth_monitor();

        // Recv-only periodic: scan for timed-out connections.
        self.handle_disconnects();

        // 4-F.naia.c.2b: per-packet `send_conn.drain_acks(&mut [])` calls
        // (Heartbeat / Ping / Pong) no longer fire here — RecvHandle must
        // be send-side-free. The consolidating per-address `drain_acks`
        // runs once in `SendState::process_recv_packets` over
        // `self.received_addresses`. See `SendState::process_recv_packets`
        // for the rationale.
        loop {
            match self.recv_io.recv_reader() {
                Ok(Some((address, owned_reader))) => {
                    let mut reader = owned_reader.borrow();

                    let Ok(header) = StandardHeader::de(&mut reader) else {
                        // Received a malformed packet
                        // TODO: increase suspicion against packet sender
                        continue;
                    };

                    self.received_addresses.insert(address);

                    match header.packet_type {
                        PacketType::Data => {
                            self.addrs_with_new_packets.insert(address);
                            if self.read_data_packet(&address, &header, &mut reader).is_err() {
                                warn!("Server Error: cannot read malformed packet");
                                continue;
                            }
                        }
                        PacketType::Heartbeat => {
                            if let Some(recv_conn) =
                                self.recv_user_connections.get_mut(&address)
                            {
                                recv_conn.process_incoming_header(&header);
                            }
                            continue;
                        }
                        PacketType::Ping => {
                            // 4-F.naia.c.1: queue the pong response on
                            // `pending_outbound_packets`; the send thread
                            // flushes it at the top of `send_all_packets`.
                            let response = self
                                .shared
                                .time_manager
                                .read()
                                .process_ping(&mut reader)
                                .unwrap();
                            self.shared
                                .pending_outbound_packets
                                .lock()
                                .push((address, response.to_packet()));

                            if let Some(recv_conn) =
                                self.recv_user_connections.get_mut(&address)
                            {
                                recv_conn.process_incoming_header(&header);
                            }
                            continue;
                        }
                        PacketType::Pong => {
                            if let Some(recv_conn) =
                                self.recv_user_connections.get_mut(&address)
                            {
                                recv_conn.process_incoming_header(&header);
                                let tm = self.shared.time_manager.read();
                                recv_conn.ping_manager.process_pong(&*tm, &mut reader);
                                // 4-F.naia.h: mirror the recv-side RTT EMA into
                                // `ConnectionShared::rtt_avg_ms` so the send half
                                // (Iris Phase 3B in `SendState::send_all_packets`)
                                // can read RTT without touching `recv_user_connections`.
                                recv_conn
                                    .shared
                                    .set_rtt_avg_ms(recv_conn.ping_manager.rtt_average);
                            }
                            continue;
                        }
                        PacketType::Handshake => {
                            let handshake_header_result = HandshakeHeader::de(&mut reader);
                            let Ok(HandshakeHeader::ClientConnectRequest) =
                                handshake_header_result
                            else {
                                warn!(
                                    "Server Error: received invalid handshake packet: {:?}",
                                    handshake_header_result
                                );
                                continue;
                            };
                            // 4-F.naia.c.1: defer `finalize_connection` to
                            // the coordination stage via `pending_handshakes`.
                            self.shared.pending_handshakes.lock().push(address);

                            // Queue the Connect Response on
                            // `pending_outbound_packets`.
                            let packet =
                                HandshakeManager::write_connect_response().to_packet();
                            self.shared
                                .pending_outbound_packets
                                .lock()
                                .push((address, packet));

                            continue;
                        }
                    }
                }
                Ok(None) => break,
                Err(error) => {
                    self.incoming_world_events
                        .push_error(NaiaServerError::Wrapped(Box::new(error)));
                }
            }
        }
    }

    /// Drain accumulated world events (step 4-F.naia.c.2b).
    pub fn take_world_events(&mut self) -> WorldEvents<E> {
        std::mem::replace(&mut self.incoming_world_events, WorldEvents::<E>::new())
    }

    /// Advance the tick clock and return any newly-fired tick events
    /// (step 4-F.naia.c.2b — recv-side equivalent of
    /// `WorldServer::take_tick_events`).
    pub fn take_tick_events(&mut self, now: &naia_shared::Instant) -> TickEvents {
        let new_tick = {
            let mut tm = self.shared.time_manager.write();
            if tm.recv_server_tick(now) {
                Some(tm.current_tick())
            } else {
                None
            }
        };
        if let Some(tick) = new_tick {
            self.incoming_tick_events.push_tick(tick);
        }
        std::mem::replace(&mut self.incoming_tick_events, TickEvents::new())
    }

    /// Recv-side periodic disconnect scan (step 4-F.naia.c.2b).
    /// Pure recv-only — touches only `self.timeout_timer`,
    /// `self.recv_user_connections`, `self.outstanding_disconnects`.
    fn handle_disconnects(&mut self) {
        if self.timeout_timer.ringing() {
            self.timeout_timer.reset();
            let mut user_disconnects: Vec<UserKey> = Vec::new();
            for (_, recv_conn) in self.recv_user_connections.iter() {
                if recv_conn.should_drop() && !recv_conn.manual_disconnect {
                    user_disconnects.push(recv_conn.user_key);
                }
            }
            for user_key in user_disconnects {
                self.outstanding_disconnects
                    .push((user_key, DisconnectReason::TimedOut));
            }
        }
    }

    /// Recv-side Data-packet bookkeeping (step 4-F.naia.c.2b).
    /// Publishes the ack snapshot via `process_incoming_header`, reads
    /// the client tick, and buffers the rest of the reader into
    /// `pending_data_packets` for `SendState::process_recv_packets` to
    /// decode at coordination stage.
    fn read_data_packet(
        &mut self,
        address: &std::net::SocketAddr,
        header: &StandardHeader,
        reader: &mut BitReader,
    ) -> Result<(), SerdeErr> {
        if header.packet_type != PacketType::Data {
            panic!("Server Error: received non-data packet in data packet handler");
        }

        let Some(recv_conn) = self.recv_user_connections.get_mut(address) else {
            return Ok(());
        };

        #[cfg(feature = "e2e_debug")]
        {
            use std::sync::atomic::Ordering;
            crate::server::world_server::SERVER_RX_FRAMES.fetch_add(1, Ordering::Relaxed);
        }

        recv_conn.process_incoming_header(header);

        let client_tick = Tick::de(reader)?;
        let owned = reader.to_owned();
        self.pending_data_packets.push((*address, client_tick, owned));

        Ok(())
    }
}

// SAFETY: All fields are Send: HashMap of RecvConnection (which is Send
// because PingManager / TickBufferReceiver / Timer / Arc<ConnectionShared>
// are all Send) plus owned timer/queue/event state.
unsafe impl<E: Copy + Eq + std::hash::Hash + Send + Sync> Send for RecvState<E> {}
