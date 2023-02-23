use std::any::Any;
use std::collections::HashMap;

use naia_shared::{Channel, ChannelKind, Message, MessageKind};

use crate::UserKey;

pub struct TickBufferMessages {
    inner: HashMap<ChannelKind, HashMap<MessageKind, Vec<(UserKey, Box<dyn Message>)>>>,
    empty: bool,
}

impl TickBufferMessages {
    pub fn new() -> Self {
        Self {
            inner: HashMap::new(),
            empty: true,
        }
    }

    pub fn push(
        &mut self,
        user_key: &UserKey,
        channel_kind: &ChannelKind,
        message: Box<dyn Message>,
    ) {
        if !self.inner.contains_key(&channel_kind) {
            self.inner.insert(*channel_kind, HashMap::new());
        }
        let channel_map = self.inner.get_mut(&channel_kind).unwrap();
        let message_type_id = message.kind();
        if !channel_map.contains_key(&message_type_id) {
            channel_map.insert(message_type_id, Vec::new());
        }
        let list = channel_map.get_mut(&message_type_id).unwrap();
        list.push((*user_key, message));
        self.empty = false;
    }

    pub fn read<C: Channel, M: Message>(&self) -> Vec<(UserKey, M)> {
        let mut output = Vec::new();

        let channel_kind = ChannelKind::of::<C>();
        if let Some(message_map) = self.inner.get(&channel_kind) {
            let message_kind = MessageKind::of::<M>();
            if let Some(messages) = message_map.get(&message_kind) {
                for (user_key, boxed_message) in messages {
                    let boxed_any = boxed_message.clone_box().to_boxed_any();
                    let message: M = Box::<dyn Any + 'static>::downcast::<M>(boxed_any)
                        .ok()
                        .map(|boxed_m| *boxed_m)
                        .unwrap();
                    output.push((*user_key, message));
                }
            }
        }

        output
    }
}
