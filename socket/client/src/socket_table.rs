//! Per-socket connection state for the miniquad WebRTC backend
//! (naia-lib/naia#193).
//!
//! The miniquad backend used to keep one process-global set of queues and
//! cells: a second `connect` overwrote the first socket's channel and reset
//! its state, so two simultaneous sockets could never coexist. Each socket now
//! owns a [`SocketState`] in a [`SocketTable`] keyed by [`SocketId`], and the
//! JS bridge echoes that id on every callback so inbound traffic routes to
//! its own socket.
//!
//! This module is deliberately free of `wasm32`-only items -- no `extern "C"`
//! imports, no `JsObject` -- so `cargo test` on the host exercises the exact
//! isolation semantics the wasm-only backend relies on. (Same rationale as
//! the `wasm_utils` precedent: a wasm-only module is a module whose tests
//! never run.)

use std::collections::{HashMap, VecDeque};

use naia_socket_shared::IdentityToken;

use crate::server_addr::ServerAddr;

/// Identifies one miniquad client socket within a [`SocketTable`].
///
/// Allocated by [`SocketTable::connect`] and echoed back by the JS bridge on
/// every inbound callback, so traffic for socket B never lands in socket A's
/// queues.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SocketId(pub u32);

/// The inbound state of one miniquad client socket.
///
/// Same slots the old process-global statics held -- identity cell, bounded
/// auth-error cell, message and error queues, server address -- now owned per
/// socket instead of shared by all of them.
pub struct SocketState {
    pub(crate) id_cell: Option<Option<IdentityToken>>,
    pub(crate) auth_error_cell: Option<Option<(u16, String)>>,
    pub(crate) message_queue: VecDeque<Box<[u8]>>,
    pub(crate) error_queue: VecDeque<String>,
    pub(crate) server_addr: ServerAddr,
}

impl SocketState {
    fn new() -> Self {
        SocketState {
            id_cell: Some(None),
            auth_error_cell: Some(None),
            message_queue: VecDeque::new(),
            error_queue: VecDeque::new(),
            server_addr: ServerAddr::Finding,
        }
    }
}

/// Owns the [`SocketState`] of every live miniquad client socket.
///
/// On wasm32 there is exactly one table, behind a `static mut` next to the
/// `extern "C"` callbacks that must stay process-global entry points; the
/// callbacks route through it by id. Everywhere else (notably host tests)
/// this is an ordinary value.
pub struct SocketTable {
    slots: HashMap<SocketId, SocketState>,
    next_id: u32,
}

impl SocketTable {
    /// An empty table with no live sockets.
    pub fn new() -> Self {
        SocketTable {
            slots: HashMap::new(),
            next_id: 0,
        }
    }

    /// Opens a fresh per-socket slot and returns its id.
    ///
    /// Opening a second socket never touches the first socket's state: that
    /// reset-on-connect was the defect (naia-lib/naia#193).
    pub fn connect(&mut self) -> SocketId {
        let id = SocketId(self.next_id);
        self.next_id = self.next_id.wrapping_add(1);
        self.slots.insert(id, SocketState::new());
        id
    }

    /// Closes a socket's slot, dropping its queued state. Returns whether a
    /// live slot was removed.
    pub fn disconnect(&mut self, id: SocketId) -> bool {
        self.slots.remove(&id).is_some()
    }

    /// Reads a live socket's state, or `None` once it is disconnected.
    pub fn get(&self, id: SocketId) -> Option<&SocketState> {
        self.slots.get(&id)
    }

    /// Mutates a live socket's state, or `None` once it is disconnected.
    pub fn get_mut(&mut self, id: SocketId) -> Option<&mut SocketState> {
        self.slots.get_mut(&id)
    }
}

impl Default for SocketTable {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod socket_isolation_tests {
    use super::{ServerAddr, SocketTable};

    /// The defect, stated as a test: opening a second socket must leave the
    /// first socket's identity, queues, and server address untouched. The old
    /// process-global statics reset all four on every `connect`.
    #[test]
    fn a_second_connect_leaves_the_first_sockets_state_untouched() {
        let mut table = SocketTable::new();

        let first = table.connect();
        {
            let state = table.get_mut(first).expect("first socket is live");
            state
                .message_queue
                .push_back(b"first-payload".to_vec().into_boxed_slice());
            state.error_queue.push_back("first-error".to_string());
            state.server_addr = ServerAddr::Finding;
        }

        let second = table.connect();
        assert_ne!(first, second, "each socket must own a distinct id");

        // The first socket's state survived the second connect intact.
        let state = table.get(first).expect("first socket is still live");
        assert_eq!(state.message_queue.len(), 1);
        assert_eq!(&state.message_queue[0][..], b"first-payload");
        assert_eq!(state.error_queue.len(), 1);
        assert_eq!(state.error_queue[0], "first-error");
        assert_eq!(state.server_addr, ServerAddr::Finding);

        // The second socket starts empty, not with the first socket's state.
        let fresh = table.get(second).expect("second socket is live");
        assert!(matches!(fresh.id_cell, Some(None)));
        assert!(fresh.message_queue.is_empty());
        assert!(fresh.error_queue.is_empty());
        assert_eq!(fresh.server_addr, ServerAddr::Finding);
    }

    /// Interleaved traffic routes to its own socket: draining one socket's
    /// queue is an ordered pop of exactly that socket's bytes.
    #[test]
    fn traffic_routes_to_its_own_socket() {
        let mut table = SocketTable::new();
        let left = table.connect();
        let right = table.connect();

        for (id, payload) in [
            (left, b"l1".as_slice()),
            (right, b"r1".as_slice()),
            (left, b"l2".as_slice()),
            (right, b"r2".as_slice()),
        ] {
            table
                .get_mut(id)
                .expect("socket is live")
                .message_queue
                .push_back(payload.to_vec().into_boxed_slice());
        }

        let state = table.get_mut(left).expect("left socket is live");
        assert_eq!(&state.message_queue.pop_front().unwrap()[..], b"l1");
        assert_eq!(&state.message_queue.pop_front().unwrap()[..], b"l2");
        assert!(state.message_queue.pop_front().is_none());

        // The right socket never saw the left socket's pops.
        let state = table.get(right).expect("right socket is live");
        assert_eq!(state.message_queue.len(), 2);
        assert_eq!(&state.message_queue[0][..], b"r1");
        assert_eq!(&state.message_queue[1][..], b"r2");
    }

    /// Disconnecting one socket drops exactly its slot: the other socket
    /// keeps receiving, and the closed one reads as gone.
    #[test]
    fn disconnecting_one_socket_leaves_the_other_live() {
        let mut table = SocketTable::new();
        let doomed = table.connect();
        let survivor = table.connect();

        table
            .get_mut(survivor)
            .expect("survivor is live")
            .message_queue
            .push_back(b"kept".to_vec().into_boxed_slice());

        assert!(table.disconnect(doomed));
        assert!(
            !table.disconnect(doomed),
            "a second disconnect reports no slot"
        );
        assert!(table.get(doomed).is_none());
        assert!(table.get_mut(doomed).is_none());

        let state = table.get(survivor).expect("survivor is still live");
        assert_eq!(state.message_queue.len(), 1);
        assert_eq!(&state.message_queue[0][..], b"kept");
    }

    /// Each socket's identity and auth-error cells are take-once *per
    /// socket*: consuming one socket's handshake result never touches the
    /// other's outstanding handshake.
    #[test]
    fn handshake_cells_are_take_once_per_socket() {
        let mut table = SocketTable::new();
        let first = table.connect();
        let second = table.connect();

        for id in [first, second] {
            let state = table.get_mut(id).expect("socket is live");
            if let Some(auth_error_cell) = &mut state.auth_error_cell {
                *auth_error_cell = Some((409, "reason".to_string()));
            }
        }

        // Consume the first socket's answer the way IdentityReceiver does:
        // an inner take reports the rejection once, then Waiting -- while
        // the second socket's answer stays outstanding.
        let answer = table
            .get_mut(first)
            .expect("first socket is live")
            .auth_error_cell
            .as_mut()
            .and_then(|cell| cell.take());
        assert!(matches!(answer, Some((409, _))));
        let again = table
            .get_mut(first)
            .expect("first socket is live")
            .auth_error_cell
            .as_mut()
            .and_then(|cell| cell.take());
        assert!(again.is_none());

        let other = table
            .get_mut(second)
            .expect("second socket is live")
            .auth_error_cell
            .as_mut()
            .and_then(|cell| cell.take());
        assert!(matches!(other, Some((409, _))));
    }
}
