use std::collections::{HashMap, VecDeque};

use log::warn;

use naia_shared::{
    ChannelKind, GlobalRequestId, GlobalResponseId, LocalResponseId, MessageContainer,
};

use crate::UserKey;

// GlobalRequestManager
pub struct GlobalRequestManager {
    map: HashMap<GlobalRequestId, (UserKey, Option<MessageContainer>)>,
    next_id: u64,
}

impl GlobalRequestManager {
    pub fn new() -> Self {
        Self {
            map: HashMap::new(),
            next_id: 0,
        }
    }

    pub(crate) fn create_request_id(&mut self, user_key: &UserKey) -> GlobalRequestId {
        let id = GlobalRequestId::new(self.next_id);
        self.next_id = self.next_id.wrapping_add(1);

        self.map.insert(id, (*user_key, None));

        id
    }

    pub(crate) fn destroy_request_id(
        &mut self,
        request_id: &GlobalRequestId,
    ) -> Option<(UserKey, MessageContainer)> {
        let (_, response_opt) = self.map.get(request_id)?;
        if response_opt.is_some() {
            let (user_key, response_opt) = self.map.remove(request_id).unwrap();
            return Some((user_key, response_opt.unwrap()));
        }
        None
    }

    pub(crate) fn receive_response(
        &mut self,
        request_id: &GlobalRequestId,
        response: MessageContainer,
    ) {
        if let Some((_, response_opt)) = self.map.get_mut(request_id) {
            *response_opt = Some(response);
        } else {
            warn!("receive_response: dropping response for unknown request_id {:?}; request was likely cancelled or the user disconnected", request_id);
        }
    }

    /// Remove all outstanding request entries for a user that has disconnected.
    /// Without this, disconnecting mid-request leaks the entry indefinitely.
    pub(crate) fn purge_user(&mut self, user_key: &UserKey) {
        self.map.retain(|_, (key, _)| key != user_key);
    }
}

/// Most unanswered requests one user may have outstanding.
///
/// Every request a user sends creates a routing entry here, and only the
/// application answering it removes that entry. An application is under no
/// obligation to answer -- it may not recognise the request, or may be waiting on
/// something slow -- so without a cap a user can make this map grow for as long
/// as the connection lasts, and `receive_requests_and_responses` being drained
/// every tick does nothing to stop it. The bound is per user so one hostile
/// connection cannot evict another's pending work.
const MAX_OUTSTANDING_RESPONSES_PER_USER: usize = 4096;

// GlobalResponseManager
pub struct GlobalResponseManager {
    map: HashMap<GlobalResponseId, (UserKey, ChannelKind, LocalResponseId)>,
    /// Per-user insertion order, used to evict oldest-first at the cap. Entries
    /// are removed from the map on their own schedule, so ids in here may already
    /// be dead; they are skipped when encountered rather than eagerly purged.
    order: HashMap<UserKey, VecDeque<GlobalResponseId>>,
    next_id: u64,
}

impl GlobalResponseManager {
    pub fn new() -> Self {
        Self {
            map: HashMap::new(),
            order: HashMap::new(),
            next_id: 0,
        }
    }

    /// Number of outstanding response ids, across all users. Test observer.
    #[cfg(test)]
    fn outstanding(&self) -> usize {
        self.map.len()
    }

    pub(crate) fn create_response_id(
        &mut self,
        user_key: &UserKey,
        channel_kind: &ChannelKind,
        local_response_id: &LocalResponseId,
    ) -> GlobalResponseId {
        let id = GlobalResponseId::new(self.next_id);
        self.next_id = self.next_id.wrapping_add(1);

        self.map
            .insert(id, (*user_key, *channel_kind, *local_response_id));

        let queue = self.order.entry(*user_key).or_default();
        queue.push_back(id);

        // Discard ids the application has already answered. They cost nothing to
        // keep except queue length, and letting them count toward the cap would
        // evict live requests early.
        while queue.front().is_some_and(|id| !self.map.contains_key(id)) {
            queue.pop_front();
        }

        // Evict this user's oldest still-live requests until they are back under
        // the cap. Dropping the routing entry makes the request unanswerable, and
        // `send_response` already models exactly that as `Undeliverable`.
        while queue.len() > MAX_OUTSTANDING_RESPONSES_PER_USER {
            let oldest = queue.pop_front().unwrap();
            self.map.remove(&oldest);
            warn!(
                "user has more than {} unanswered requests outstanding; dropping the oldest. \
                 Responding to it will now report Undeliverable.",
                MAX_OUTSTANDING_RESPONSES_PER_USER
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
    ) -> Option<(UserKey, ChannelKind, LocalResponseId)> {
        self.map.get(global_response_id).cloned()
    }

    pub(crate) fn destroy_response_id(
        &mut self,
        global_response_id: &GlobalResponseId,
    ) -> Option<(UserKey, ChannelKind, LocalResponseId)> {
        self.map.remove(global_response_id)
    }

    /// Remove all outstanding response entries for a user that has disconnected.
    pub(crate) fn purge_user(&mut self, user_key: &UserKey) {
        self.map.retain(|_, (key, _, _)| key != user_key);
        self.order.remove(user_key);
    }
}

/// Why a `send_response` did or did not enqueue.
///
/// The distinction is load-bearing for any caller that holds refused responses and
/// retries them in order: `Backpressured` must be retried (the `ResponseSendKey` is
/// still valid and the requester is still waiting), while `Undeliverable` must be
/// dropped, or it head-of-line blocks every response queued behind it forever.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResponseSendOutcome {
    /// Enqueued on the reliable channel; the response id has been consumed.
    Sent,
    /// The channel's send queue is at its depth cap. Nothing was enqueued, the key
    /// is still valid — hold the response and retry on a later frame.
    Backpressured,
    /// Can never be delivered (the user disconnected, or the response id is no
    /// longer routable because it was already answered). Discard it.
    Undeliverable,
}

#[cfg(test)]
mod tests {
    use naia_shared::{BigMapKey, LocalRequestId};

    use super::*;

    fn channel() -> ChannelKind {
        ChannelKind::of::<naia_shared::default_channels::UnorderedReliableChannel>()
    }

    fn response_id(i: u16) -> LocalResponseId {
        LocalRequestId::from(i).receive_from_remote()
    }

    /// A user's requests only leave this map when the application answers them,
    /// and nothing obliges an application to answer. The per-tick drain of
    /// `incoming_requests` does not help: each drained request has already
    /// created a routing entry that outlives the tick.
    #[test]
    fn unanswered_requests_from_one_user_cannot_grow_without_bound() {
        let mut manager = GlobalResponseManager::new();
        let user = UserKey::from_u64(1);

        for i in 0..(MAX_OUTSTANDING_RESPONSES_PER_USER as u16 * 8) {
            manager.create_response_id(&user, &channel(), &response_id(i));
        }

        assert_eq!(manager.outstanding(), MAX_OUTSTANDING_RESPONSES_PER_USER);
        assert_eq!(
            manager.order.get(&user).map(|q| q.len()),
            Some(MAX_OUTSTANDING_RESPONSES_PER_USER),
        );
    }

    /// Eviction is oldest-first, and an evicted request degrades into the
    /// `Undeliverable` case `send_response` already handles: its routing is gone,
    /// so `peek_response_id` reports it as unanswerable rather than silently
    /// mis-routing a reply.
    #[test]
    fn the_oldest_unanswered_request_is_the_one_dropped() {
        let mut manager = GlobalResponseManager::new();
        let user = UserKey::from_u64(1);

        let oldest = manager.create_response_id(&user, &channel(), &response_id(0));
        for i in 1..=(MAX_OUTSTANDING_RESPONSES_PER_USER as u16) {
            manager.create_response_id(&user, &channel(), &response_id(i));
        }

        assert!(
            manager.peek_response_id(&oldest).is_none(),
            "the oldest request is no longer routable"
        );
    }

    /// The bound is per user: one user flooding requests must not evict the
    /// pending work of a well-behaved user sharing the server.
    #[test]
    fn one_user_flooding_does_not_evict_another_users_request() {
        let mut manager = GlobalResponseManager::new();
        let quiet = UserKey::from_u64(1);
        let flooder = UserKey::from_u64(2);

        let quiet_request = manager.create_response_id(&quiet, &channel(), &response_id(0));
        for i in 0..(MAX_OUTSTANDING_RESPONSES_PER_USER as u16 * 4) {
            manager.create_response_id(&flooder, &channel(), &response_id(i));
        }

        assert!(
            manager.peek_response_id(&quiet_request).is_some(),
            "the quiet user's request survives the flood"
        );
    }

    /// Answered requests must not count toward the cap, or a user who answers
    /// promptly would still have live requests evicted. The ordering queue is
    /// cleaned lazily, so it must also not grow without bound on this path.
    #[test]
    fn answered_requests_do_not_consume_the_cap() {
        let mut manager = GlobalResponseManager::new();
        let user = UserKey::from_u64(1);

        for i in 0..(MAX_OUTSTANDING_RESPONSES_PER_USER as u16 * 8) {
            let id = manager.create_response_id(&user, &channel(), &response_id(i));
            manager.destroy_response_id(&id);
        }

        assert_eq!(manager.outstanding(), 0);
        assert!(
            manager.order.get(&user).map(|q| q.len()).unwrap_or(0) <= 1,
            "the ordering queue does not accumulate answered ids"
        );
    }

    /// A disconnect must drop the ordering queue too, not just the routing map.
    #[test]
    fn purging_a_user_clears_the_ordering_queue() {
        let mut manager = GlobalResponseManager::new();
        let user = UserKey::from_u64(1);

        for i in 0..64u16 {
            manager.create_response_id(&user, &channel(), &response_id(i));
        }
        manager.purge_user(&user);

        assert_eq!(manager.outstanding(), 0);
        assert!(!manager.order.contains_key(&user));
    }
}
