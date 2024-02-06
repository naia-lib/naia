use naia_shared::{ChannelKind, GlobalRequestId, GlobalResponseId, LocalResponseId, MessageContainer, MessageKind};

use crate::UserKey;

// GlobalRequestManager
pub struct GlobalRequestManager {

}

impl GlobalRequestManager {
    pub fn new() -> Self {
        Self {

        }
    }

    pub(crate) fn create_request_id(&self, user_key: &UserKey, channel_kind: &ChannelKind) -> GlobalRequestId {
        todo!()
    }

    pub(crate) fn destroy_request_id(&self, request_id: &GlobalRequestId) -> Option<(UserKey, ChannelKind)> {
        todo!()
    }
}

// GlobalResponseManager
pub struct GlobalResponseManager {

}

impl GlobalResponseManager {
    pub fn new() -> Self {
        Self {

        }
    }

    pub(crate) fn create_response_id(&mut self, channel_kind: &ChannelKind, message_kind: &MessageKind, local_request_id: &LocalResponseId) -> GlobalResponseId {
        todo!()
    }

    pub(crate) fn destroy_response_id(&self, response_id: &GlobalResponseId) -> Option<MessageContainer> {
        todo!()
    }
}