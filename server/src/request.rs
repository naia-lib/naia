use std::collections::HashMap;

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

// GlobalResponseManager
pub struct GlobalResponseManager {
    map: HashMap<GlobalResponseId, (UserKey, ChannelKind, LocalResponseId)>,
    next_id: u64,
}

impl GlobalResponseManager {
    pub fn new() -> Self {
        Self {
            map: HashMap::new(),
            next_id: 0,
        }
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
