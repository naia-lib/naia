use naia_shared::{ChannelKind, MessageContainer};

// GlobalRequestManager
pub struct GlobalRequestManager {

}

impl GlobalRequestManager {
    pub fn new() -> Self {
        Self {

        }
    }

    pub(crate) fn create_request_id(&mut self, channel_kind: &ChannelKind) -> u64 {
        todo!()
    }

    pub(crate) fn destroy_request_id(&mut self, request_id: &u64) -> Option<ChannelKind> {
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

    pub(crate) fn destroy_response_id(&self, response_id: &u64) -> Option<MessageContainer> {
        todo!()
    }
}