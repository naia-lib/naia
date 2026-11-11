use std::collections::{HashMap, HashSet, VecDeque};
use std::time::Duration;

use log::warn;

use naia_shared::{
    CancelDisposition, ChannelKind, ConnectionRequestNonce, GameInstant, GlobalRequestId,
    GlobalResponseId, LocalResponseId, MessageContainer, NonceAllocator, NonceExhaustion,
};

/// Non-destructive read of a request slot's state.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum SlotPoll {
    /// The request is live and no terminal state arrived yet.
    Pending,
    /// A response arrived and waits to be taken.
    Ready,
    /// No response will ever arrive: the request was cancelled, its
    /// exchange abandoned, or the key names nothing live.
    Abandoned,
    /// The request's deadline passed with no response (messaging-24 t2).
    /// Reported, never silent: the application can tell expiry apart from
    /// cancellation.
    TimedOut,
}

/// One live outgoing request: its checked H3 nonce, an optional
/// arrived-but-untaken response, and an optional deadline (messaging-24).
struct RequestSlot {
    nonce: ConnectionRequestNonce,
    response: Option<MessageContainer>,
    deadline: Option<GameInstant>,
}

impl RequestSlot {
    /// True when the slot holds no response and `now` is past its deadline.
    /// A slot holding a response is never expired: the response arrived
    /// within the timeout, so the exchange stays receivable after it.
    fn is_expired(&self, now: GameInstant) -> bool {
        self.response.is_none() && self.deadline.is_some_and(|at| now.is_more_than(&at))
    }
}

// GlobalRequestManager
pub struct GlobalRequestManager {
    slots: HashMap<GlobalRequestId, RequestSlot>,
    /// Nonces whose exchanges are over by cancel or abandonment. Consulted
    /// for duplicate suppression and for polling cancelled keys; dropped
    /// with the connection, which owns this manager.
    abandoned: HashSet<ConnectionRequestNonce>,
    nonces: NonceAllocator,
    next_id: u64,
}

impl GlobalRequestManager {
    pub fn new() -> Self {
        Self {
            slots: HashMap::new(),
            abandoned: HashSet::new(),
            nonces: NonceAllocator::new(),
            next_id: 0,
        }
    }

    /// Starts the nonce supply at `next_nonce`. Test hook: names where a
    /// fresh supply begins (for the exhaustion pin), never resumes a live one.
    #[cfg(test)]
    pub(crate) fn with_nonce_start(next_nonce: u64) -> Self {
        Self {
            slots: HashMap::new(),
            abandoned: HashSet::new(),
            nonces: NonceAllocator::with_next(next_nonce),
            next_id: 0,
        }
    }

    /// Number of live request slots. Test observer: the H3 acceptance
    /// requires routing tables to return to baseline.
    #[cfg(test)]
    pub(crate) fn outstanding(&self) -> usize {
        self.slots.len()
    }

    /// Allocates a request id with a checked, never-reused H3 nonce.
    ///
    /// H3: nonce exhaustion is an error the caller handles by retiring the
    /// connection, never by aliasing a live nonce — so this fails rather
    /// than wrapping. The nonce is handed back alongside the id so the
    /// caller can name the exchange on the wire (envelope cutover).
    ///
    /// `timeout` arms the messaging-24 deadline from `now`; `None` waits
    /// without a deadline.
    pub(crate) fn create_request_id(
        &mut self,
        timeout: Option<Duration>,
        now: GameInstant,
    ) -> Result<(GlobalRequestId, ConnectionRequestNonce), NonceExhaustion> {
        let nonce = self.nonces.next()?;
        let id = GlobalRequestId::new(self.next_id);
        self.next_id = self.next_id.wrapping_add(1);

        let deadline = timeout.map(|limit| {
            let millis = u32::try_from(limit.as_millis()).unwrap_or(u32::MAX);
            now.add_millis(millis)
        });
        self.slots.insert(
            id,
            RequestSlot {
                nonce,
                response: None,
                deadline,
            },
        );

        Ok((id, nonce))
    }

    /// Check if a response is available for the given request ID (non-destructive)
    pub(crate) fn has_response(&self, request_id: &GlobalRequestId) -> bool {
        self.slots
            .get(request_id)
            .is_some_and(|slot| slot.response.is_some())
    }

    /// Non-destructive read of a slot: live with a response is Ready, live
    /// past its deadline without one is TimedOut, live otherwise is
    /// Pending, anything else is Abandoned — a cancelled,
    /// completed-and-taken, or never-allocated key will never yield a
    /// response again. Call [`reap_expired`](Self::reap_expired) first so a
    /// reported timeout also releases tracking.
    pub(crate) fn poll_slot(&self, request_id: &GlobalRequestId, now: GameInstant) -> SlotPoll {
        match self.slots.get(request_id) {
            Some(slot) if slot.response.is_some() => SlotPoll::Ready,
            Some(slot) if slot.is_expired(now) => SlotPoll::TimedOut,
            Some(_) => SlotPoll::Pending,
            None => SlotPoll::Abandoned,
        }
    }

    /// Removes every expired response-less slot and tombstones its nonce
    /// (messaging-24 t4): tracking returns to baseline, and a late
    /// response for a reaped exchange drops on the unknown-id path instead
    /// of resurrecting the slot. Slots holding a response survive — the
    /// response arrived within the timeout and stays receivable.
    pub(crate) fn reap_expired(&mut self, now: GameInstant) {
        let mut expired = Vec::new();
        for (id, slot) in &self.slots {
            if slot.is_expired(now) {
                expired.push(*id);
            }
        }
        for id in expired {
            if let Some(slot) = self.slots.remove(&id) {
                self.abandoned.insert(slot.nonce);
            }
        }
    }

    /// Cancels a live request: removes its routing entry and marks its
    /// transport nonce abandoned, so a late response for it drops on the
    /// unknown-id path instead of resurrecting the slot.
    pub(crate) fn cancel_request(&mut self, request_id: &GlobalRequestId) -> CancelDisposition {
        match self.slots.remove(request_id) {
            Some(slot) => {
                self.abandoned.insert(slot.nonce);
                CancelDisposition::Cancelled { nonce: slot.nonce }
            }
            None => CancelDisposition::UnknownKey,
        }
    }

    /// Drops a slot whose request never reached the wire: plain removal,
    /// with NO abandonment tombstone. The peer never saw the exchange, so
    /// there is nothing to suppress — and the spent nonce stays spent,
    /// never recycled, so a live nonce is never aliased.
    pub(crate) fn discard_unsent_request(&mut self, request_id: &GlobalRequestId) {
        self.slots.remove(request_id);
    }

    /// Non-destructive read of an arrived response. The slot keeps its
    /// copy; take it with [`destroy_request_id`](Self::destroy_request_id).
    pub(crate) fn peek_request(&self, request_id: &GlobalRequestId) -> Option<MessageContainer> {
        self.slots.get(request_id)?.response.clone()
    }

    pub(crate) fn destroy_request_id(
        &mut self,
        request_id: &GlobalRequestId,
    ) -> Option<MessageContainer> {
        self.slots.get(request_id)?.response.as_ref()?;
        self.slots.remove(request_id)?.response
    }

    pub(crate) fn receive_response(
        &mut self,
        request_id: &GlobalRequestId,
        response: MessageContainer,
        now: GameInstant,
    ) {
        let Some(slot) = self.slots.get_mut(request_id) else {
            warn!("receive_response: dropping response for unknown request_id {request_id:?}; request was likely cancelled or the connection was reset");
            return;
        };
        if slot.is_expired(now) {
            // Late for a timed-out exchange (messaging-24 t2): reap the
            // slot and drop the response rather than landing it in a
            // dead exchange.
            let nonce = slot.nonce;
            self.slots.remove(request_id);
            self.abandoned.insert(nonce);
            warn!("receive_response: dropping late response for timed-out request_id {request_id:?}; the request was already cancelled by its timeout");
            return;
        }
        slot.response = Some(response);
    }
}

/// Most unanswered requests the server may have outstanding against this client.
///
/// Every request the server sends creates a routing entry here, and only the
/// application answering it removes that entry. An application is under no
/// obligation to answer, so without a cap the map grows for as long as the
/// connection lasts. The client has a single peer, so the bound is global.
const MAX_OUTSTANDING_RESPONSES: usize = 4096;

// GlobalResponseManager
pub struct GlobalResponseManager {
    /// H3: the routing keeps the incoming request's wire nonce, so
    /// `send_response` echoes it and the requester resolves by
    /// (local id, nonce).
    map: HashMap<GlobalResponseId, (ChannelKind, LocalResponseId, ConnectionRequestNonce)>,
    /// Insertion order, used to evict oldest-first at the cap. Ids here may
    /// already have been answered and removed from the map; they are skipped when
    /// encountered rather than eagerly purged.
    order: VecDeque<GlobalResponseId>,
    next_id: u64,
}

impl GlobalResponseManager {
    pub fn new() -> Self {
        Self {
            map: HashMap::new(),
            order: VecDeque::new(),
            next_id: 0,
        }
    }

    /// Number of outstanding response ids. Test observer.
    #[cfg(test)]
    fn outstanding(&self) -> usize {
        self.map.len()
    }

    pub(crate) fn create_response_id(
        &mut self,
        channel_kind: &ChannelKind,
        local_response_id: &LocalResponseId,
        nonce: ConnectionRequestNonce,
    ) -> GlobalResponseId {
        let id = GlobalResponseId::new(self.next_id);
        self.next_id = self.next_id.wrapping_add(1);

        self.map
            .insert(id, (*channel_kind, *local_response_id, nonce));
        self.order.push_back(id);

        // Discard ids the application has already answered, so they do not count
        // toward the cap and evict live requests early.
        while self
            .order
            .front()
            .is_some_and(|id| !self.map.contains_key(id))
        {
            self.order.pop_front();
        }

        // Evict the oldest still-live requests. Dropping the routing entry makes
        // the request unanswerable, which `send_response` reports as
        // `Undeliverable`.
        while self.order.len() > MAX_OUTSTANDING_RESPONSES {
            let Some(oldest) = self.order.pop_front() else {
                break;
            };
            self.map.remove(&oldest);
            warn!(
                "server has more than {MAX_OUTSTANDING_RESPONSES} unanswered requests outstanding; dropping the oldest. \
                 Responding to it will now report Undeliverable."
            );
        }

        id
    }

    /// Look up a response id's routing WITHOUT consuming it.
    ///
    /// Sending a response can be refused (the reliable channel's queue-depth cap),
    /// and a refused send must stay retryable — so the mapping is only destroyed
    /// once the enqueue actually succeeds.
    pub(crate) fn peek_response_id(
        &self,
        global_response_id: &GlobalResponseId,
    ) -> Option<(ChannelKind, LocalResponseId, ConnectionRequestNonce)> {
        self.map.get(global_response_id).copied()
    }

    pub(crate) fn destroy_response_id(
        &mut self,
        global_response_id: &GlobalResponseId,
    ) -> Option<(ChannelKind, LocalResponseId, ConnectionRequestNonce)> {
        self.map.remove(global_response_id)
    }
}

#[cfg(test)]
mod tests {
    use naia_shared::LocalRequestId;

    use super::*;

    fn channel() -> ChannelKind {
        ChannelKind::of::<naia_shared::default_channels::UnorderedReliableChannel>()
    }

    fn response_id(i: u16) -> LocalResponseId {
        LocalRequestId::from(i).receive_from_remote()
    }

    /// The client's peer is the server, and a server that sends requests the
    /// application never answers grows this map for the life of the connection.
    #[test]
    fn unanswered_requests_cannot_grow_without_bound() {
        let mut manager = GlobalResponseManager::new();

        for i in 0..(MAX_OUTSTANDING_RESPONSES as u16 * 8) {
            manager.create_response_id(
                &channel(),
                &response_id(i),
                ConnectionRequestNonce::from_wire(i as u64),
            );
        }

        assert_eq!(manager.outstanding(), MAX_OUTSTANDING_RESPONSES);
        assert_eq!(manager.order.len(), MAX_OUTSTANDING_RESPONSES);
    }

    /// Eviction is oldest-first, and an evicted request becomes unroutable rather
    /// than silently mis-routing a reply.
    #[test]
    fn the_oldest_unanswered_request_is_the_one_dropped() {
        let mut manager = GlobalResponseManager::new();

        let oldest = manager.create_response_id(
            &channel(),
            &response_id(0),
            ConnectionRequestNonce::from_wire(0),
        );
        for i in 1..=(MAX_OUTSTANDING_RESPONSES as u16) {
            manager.create_response_id(
                &channel(),
                &response_id(i),
                ConnectionRequestNonce::from_wire(i as u64),
            );
        }

        assert!(manager.peek_response_id(&oldest).is_none());
    }

    /// Answered requests must not consume the cap, and the lazily-cleaned
    /// ordering queue must not accumulate them.
    #[test]
    fn answered_requests_do_not_consume_the_cap() {
        let mut manager = GlobalResponseManager::new();

        for i in 0..(MAX_OUTSTANDING_RESPONSES as u16 * 8) {
            let id = manager.create_response_id(
                &channel(),
                &response_id(i),
                ConnectionRequestNonce::from_wire(i as u64),
            );
            manager.destroy_response_id(&id);
        }

        assert_eq!(manager.outstanding(), 0);
        assert!(manager.order.len() <= 1);
    }

    // H3 request-abandonment producer (LOCAL_GUEST_PLAN) — the plan's
    // acceptance selector is `cargo test -p naia-client --lib
    // local_guest_abandonment`.
    use naia_shared::Message;

    #[derive(Message)]
    struct ProbeMessage {
        value: u8,
    }

    fn probe_container() -> MessageContainer {
        MessageContainer::new(Box::new(ProbeMessage { value: 7 }))
    }

    fn request_manager() -> GlobalRequestManager {
        GlobalRequestManager::new()
    }

    #[test]
    fn local_guest_abandonment_cancel_removes_slot_and_names_nonce() {
        let mut manager = request_manager();
        let (id, _) = manager
            .create_request_id(None, test_start())
            .expect("capacity remains");
        assert_eq!(manager.poll_slot(&id, test_start()), SlotPoll::Pending);

        let disposition = manager.cancel_request(&id);
        let CancelDisposition::Cancelled { nonce } = disposition else {
            panic!("first cancel must succeed, got {:?}", disposition);
        };
        assert_eq!(nonce.value(), 0);
        assert!(!manager.has_response(&id));
        assert_eq!(manager.poll_slot(&id, test_start()), SlotPoll::Abandoned);
    }

    #[test]
    fn discard_unsent_request_drops_the_slot_without_abandoning() {
        // A request the queue refuses never reached the wire: its slot
        // must vanish, but no abandonment tombstone may record it — the
        // peer never saw the exchange, so there is nothing to suppress.
        let mut manager = request_manager();
        let (id, first_nonce) = manager
            .create_request_id(None, test_start())
            .expect("capacity remains");
        assert_eq!(manager.outstanding(), 1);

        manager.discard_unsent_request(&id);
        assert_eq!(manager.outstanding(), 0);
        assert_eq!(manager.poll_slot(&id, test_start()), SlotPoll::Abandoned);
        assert!(
            manager.abandoned.is_empty(),
            "an unsent request leaves no tombstone: the peer never saw it"
        );

        // The spent nonce stays spent: the next allocation moves forward,
        // never back onto the discarded exchange.
        let (_, second_nonce) = manager
            .create_request_id(None, test_start())
            .expect("capacity remains");
        assert_ne!(
            second_nonce.value(),
            first_nonce.value(),
            "a discarded exchange's nonce must never be reissued"
        );
    }

    #[test]
    fn local_guest_abandonment_second_cancel_is_unknown_key() {
        let mut manager = request_manager();
        let (id, _) = manager
            .create_request_id(None, test_start())
            .expect("capacity remains");
        assert!(matches!(
            manager.cancel_request(&id),
            CancelDisposition::Cancelled { .. }
        ));
        assert_eq!(manager.cancel_request(&id), CancelDisposition::UnknownKey);
    }

    #[test]
    fn local_guest_abandonment_poll_pending_then_ready() {
        let mut manager = request_manager();
        let (id, _) = manager
            .create_request_id(None, test_start())
            .expect("capacity remains");
        assert_eq!(manager.poll_slot(&id, test_start()), SlotPoll::Pending);

        manager.receive_response(&id, probe_container(), test_start());
        assert_eq!(manager.poll_slot(&id, test_start()), SlotPoll::Ready);
    }

    #[test]
    fn local_guest_abandonment_late_response_after_cancel_drops() {
        let mut manager = request_manager();
        let (id, _) = manager
            .create_request_id(None, test_start())
            .expect("capacity remains");
        manager.cancel_request(&id);

        manager.receive_response(&id, probe_container(), test_start());
        assert!(!manager.has_response(&id));
        assert_eq!(manager.poll_slot(&id, test_start()), SlotPoll::Abandoned);
    }

    #[test]
    fn local_guest_abandonment_exhaustion_is_checked() {
        let mut manager = GlobalRequestManager::with_nonce_start(u64::MAX);
        let (last, _) = manager
            .create_request_id(None, test_start())
            .expect("u64::MAX allocatable");
        assert!(manager.create_request_id(None, test_start()).is_err());
        // No wrap: the failed create allocated nothing.
        assert_eq!(manager.outstanding(), 1);
        assert_eq!(manager.poll_slot(&last, test_start()), SlotPoll::Pending);
    }

    /// H3 envelope cutover: the response routing keeps the incoming
    /// request's wire nonce, so `send_response` echoes the nonce the
    /// requester resolves on. A routing that dropped it would answer with
    /// a nonce no outstanding exchange names.
    #[test]
    fn response_routing_keeps_the_wire_nonce() {
        let mut manager = GlobalResponseManager::new();
        let nonce = ConnectionRequestNonce::from_wire(41);
        let id = manager.create_response_id(&channel(), &response_id(3), nonce);

        let (_, _, kept) = manager.peek_response_id(&id).expect("a live routing peeks");
        assert_eq!(kept, nonce);
    }

    #[test]
    fn local_guest_abandonment_routing_returns_to_baseline() {
        let mut manager = request_manager();
        let mut ids = Vec::new();
        for _ in 0..64 {
            let (id, _) = manager
                .create_request_id(None, test_start())
                .expect("capacity remains");
            ids.push(id);
        }
        for (i, id) in ids.iter().enumerate() {
            if i % 2 == 0 {
                manager.cancel_request(id);
            } else {
                manager.receive_response(id, probe_container(), test_start());
                manager.destroy_request_id(id);
            }
        }
        assert_eq!(manager.outstanding(), 0);
    }

    use std::time::Duration;

    use naia_shared::GameInstant;

    /// Fixed test clock: `GameInstant::from_millis` needs no running clock,
    /// so deadlines stay deterministic under `test_time` and wall alike.
    fn test_start() -> GameInstant {
        GameInstant::from_millis(1_000)
    }

    fn millis_after(start: GameInstant, millis: u32) -> GameInstant {
        start.add_millis(millis)
    }

    /// messaging-24 t1: an unanswered request past its deadline polls
    /// TimedOut — reported, never silent.
    #[test]
    fn expired_request_polls_timed_out() {
        let mut manager = request_manager();
        let start = test_start();
        let (id, _) = manager
            .create_request_id(Some(Duration::from_millis(1)), start)
            .expect("capacity remains");

        assert_eq!(
            manager.poll_slot(&id, millis_after(start, 30_000)),
            SlotPoll::TimedOut
        );
    }

    /// messaging-24 t4: reaping an expired slot releases tracking and leaves
    /// a tombstone, so a late response still drops on the unknown-id path.
    #[test]
    fn reap_expired_removes_slot_and_tombstones_nonce() {
        let mut manager = request_manager();
        let start = test_start();
        let (id, _) = manager
            .create_request_id(Some(Duration::from_millis(1)), start)
            .expect("capacity remains");

        let later = millis_after(start, 30_000);
        manager.reap_expired(later);

        assert_eq!(manager.outstanding(), 0);
        assert_eq!(manager.poll_slot(&id, later), SlotPoll::Abandoned);
        manager.receive_response(&id, probe_container(), later);
        assert!(!manager.has_response(&id));
    }

    /// A response that arrives BEFORE the deadline is still receivable after
    /// it: the timeout cancels unanswered requests, not delivered ones.
    #[test]
    fn response_arriving_before_deadline_survives_it() {
        let mut manager = request_manager();
        let start = test_start();
        let (id, _) = manager
            .create_request_id(Some(Duration::from_secs(30)), start)
            .expect("capacity remains");

        manager.receive_response(&id, probe_container(), start);
        assert_eq!(
            manager.poll_slot(&id, millis_after(start, 31_000)),
            SlotPoll::Ready
        );
        assert!(manager.has_response(&id));
    }

    /// messaging-24 t2: a response arriving after the deadline never lands —
    /// even when the slot has not been reaped yet, the receive path drops it.
    #[test]
    fn late_response_after_deadline_is_dropped_without_reap() {
        let mut manager = request_manager();
        let start = test_start();
        let (id, _) = manager
            .create_request_id(Some(Duration::from_millis(1)), start)
            .expect("capacity remains");

        let later = millis_after(start, 30_000);
        manager.receive_response(&id, probe_container(), later);

        assert!(!manager.has_response(&id));
        assert_eq!(manager.poll_slot(&id, later), SlotPoll::Abandoned);
        assert_eq!(manager.outstanding(), 0);
    }

    /// Cancel wins over expiry: an explicitly cancelled key polls Abandoned,
    /// never TimedOut, however late it is polled.
    #[test]
    fn cancelled_slot_polls_abandoned_not_timed_out() {
        let mut manager = request_manager();
        let start = test_start();
        let (id, _) = manager
            .create_request_id(Some(Duration::from_millis(1)), start)
            .expect("capacity remains");
        manager.cancel_request(&id);

        assert_eq!(
            manager.poll_slot(&id, millis_after(start, 30_000)),
            SlotPoll::Abandoned
        );
    }

    /// A request with no timeout never expires, however late it is polled.
    #[test]
    fn request_without_timeout_never_expires() {
        let mut manager = request_manager();
        let start = test_start();
        let (id, _) = manager
            .create_request_id(None, start)
            .expect("capacity remains");

        assert_eq!(
            manager.poll_slot(&id, millis_after(start, 1_000_000)),
            SlotPoll::Pending
        );
    }
}
