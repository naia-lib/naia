use std::hash::Hash;
use crate::{EntityAction, MessageId, OrderedReliableReceiver, ProtocolKindType};

pub struct EntityActionReceiver<E: Copy + Hash, K: ProtocolKindType> {
    inner: OrderedReliableReceiver<EntityAction<E, K>>,
}

impl<E: Copy + Hash, K: ProtocolKindType> EntityActionReceiver<E, K> {
    pub fn new() -> Self {
        Self {
            inner: OrderedReliableReceiver::new(),
        }
    }

    pub fn buffer_message(&mut self, action_id: MessageId, action: EntityAction<E, K>) {
        return self.inner.buffer_message(action_id, action);
    }

    pub fn receive_messages(&mut self) -> Vec<EntityAction<E, K>> {
        return self.inner.receive_messages();
    }
}