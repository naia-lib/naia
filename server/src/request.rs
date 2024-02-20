use std::collections::HashMap;

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

        self.map.insert(id, (user_key.clone(), None));

        id
    }

    pub(crate) fn destroy_request_id(
        &mut self,
        request_id: &GlobalRequestId,
    ) -> Option<(UserKey, MessageContainer)> {
        let Some((_, response_opt)) = self.map.get(request_id) else {
            return None;
        };
        if response_opt.is_some() {
            let (user_key, response_opt) = self.map.remove(request_id).unwrap();
            return Some((user_key, response_opt.unwrap()));
        }
        return None;
    }

    pub(crate) fn receive_response(
        &mut self,
        request_id: &GlobalRequestId,
        response: MessageContainer,
    ) {
        let (_, response_opt) = self.map.get_mut(request_id).unwrap();
        *response_opt = Some(response);
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

        self.map.insert(
            id,
            (
                user_key.clone(),
                channel_kind.clone(),
                local_response_id.clone(),
            ),
        );

        id
    }

    pub(crate) fn destroy_response_id(
        &mut self,
        global_response_id: &GlobalResponseId,
    ) -> Option<(UserKey, ChannelKind, LocalResponseId)> {
        self.map.remove(global_response_id)
    }
}
