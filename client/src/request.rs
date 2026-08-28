use std::collections::{HashMap, VecDeque};

use log::warn;

use naia_shared::{
    ChannelKind, GlobalRequestId, GlobalResponseId, LocalResponseId, MessageContainer,
};

// GlobalRequestManager
pub struct GlobalRequestManager {
    map: HashMap<GlobalRequestId, Option<MessageContainer>>,
    next_id: u64,
}

impl GlobalRequestManager {
    pub fn new() -> Self {
        Self {
            map: HashMap::new(),
            next_id: 0,
        }
    }

    pub(crate) fn create_request_id(&mut self) -> GlobalRequestId {
        let id = GlobalRequestId::new(self.next_id);
        self.next_id = self.next_id.wrapping_add(1);

        self.map.insert(id, None);

        id
    }

    /// Check if a response is available for the given request ID (non-destructive)
    pub(crate) fn has_response(&self, request_id: &GlobalRequestId) -> bool {
        self.map
            .get(request_id)
            .map(|opt| opt.is_some())
            .unwrap_or(false)
    }

    pub(crate) fn destroy_request_id(
        &mut self,
        request_id: &GlobalRequestId,
    ) -> Option<MessageContainer> {
        let response_opt = self.map.get(request_id)?;
        if response_opt.is_some() {
            let response_opt = self.map.remove(request_id).unwrap();
            return Some(response_opt.unwrap());
        }
        None
    }

    pub(crate) fn receive_response(
        &mut self,
        request_id: &GlobalRequestId,
        response: MessageContainer,
    ) {
        if let Some(response_opt) = self.map.get_mut(request_id) {
            *response_opt = Some(response);
        } else {
            warn!("receive_response: dropping response for unknown request_id {:?}; request was likely cancelled or the connection was reset", request_id);
        }
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
    map: HashMap<GlobalResponseId, (ChannelKind, LocalResponseId)>,
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
    ) -> GlobalResponseId {
        let id = GlobalResponseId::new(self.next_id);
        self.next_id = self.next_id.wrapping_add(1);

        self.map.insert(id, (*channel_kind, *local_response_id));
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
            let oldest = self.order.pop_front().unwrap();
            self.map.remove(&oldest);
            warn!(
                "server has more than {} unanswered requests outstanding; dropping the oldest. \
                 Responding to it will now report Undeliverable.",
                MAX_OUTSTANDING_RESPONSES
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
    ) -> Option<(ChannelKind, LocalResponseId)> {
        self.map.get(global_response_id).cloned()
    }

    pub(crate) fn destroy_response_id(
        &mut self,
        global_response_id: &GlobalResponseId,
    ) -> Option<(ChannelKind, LocalResponseId)> {
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
            manager.create_response_id(&channel(), &response_id(i));
        }

        assert_eq!(manager.outstanding(), MAX_OUTSTANDING_RESPONSES);
        assert_eq!(manager.order.len(), MAX_OUTSTANDING_RESPONSES);
    }

    /// Eviction is oldest-first, and an evicted request becomes unroutable rather
    /// than silently mis-routing a reply.
    #[test]
    fn the_oldest_unanswered_request_is_the_one_dropped() {
        let mut manager = GlobalResponseManager::new();

        let oldest = manager.create_response_id(&channel(), &response_id(0));
        for i in 1..=(MAX_OUTSTANDING_RESPONSES as u16) {
            manager.create_response_id(&channel(), &response_id(i));
        }

        assert!(manager.peek_response_id(&oldest).is_none());
    }

    /// Answered requests must not consume the cap, and the lazily-cleaned
    /// ordering queue must not accumulate them.
    #[test]
    fn answered_requests_do_not_consume_the_cap() {
        let mut manager = GlobalResponseManager::new();

        for i in 0..(MAX_OUTSTANDING_RESPONSES as u16 * 8) {
            let id = manager.create_response_id(&channel(), &response_id(i));
            manager.destroy_response_id(&id);
        }

        assert_eq!(manager.outstanding(), 0);
        assert!(manager.order.len() <= 1);
    }
}
