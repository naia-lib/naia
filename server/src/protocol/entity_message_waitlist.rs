use naia_shared::{ChannelIndex, KeyGenerator, MessageManager, Protocolize};
use std::{
    collections::{HashMap, HashSet},
    hash::Hash,
};

type MessageHandle = u16;

pub struct EntityMessageWaitlist<P: Protocolize, E: Copy + Eq + Hash, C: ChannelIndex> {
    message_handle_store: KeyGenerator<MessageHandle>,
    messages: HashMap<MessageHandle, (Vec<E>, C, P)>,
    waiting_entities: HashMap<E, HashSet<MessageHandle>>,
    in_scope_entities: HashSet<E>,
    ready_messages: Vec<(C, P)>,
}

impl<P: Protocolize, E: Copy + Eq + Hash, C: ChannelIndex> Default
    for EntityMessageWaitlist<P, E, C>
{
    fn default() -> Self {
        Self {
            messages: HashMap::default(),
            message_handle_store: KeyGenerator::default(),
            waiting_entities: HashMap::default(),
            in_scope_entities: HashSet::default(),
            ready_messages: Vec::default(),
        }
    }
}

impl<P: Protocolize, E: Copy + Eq + Hash, C: ChannelIndex> EntityMessageWaitlist<P, E, C> {
    pub fn queue_message(&mut self, entities: Vec<E>, channel: C, message: P) {
        let new_handle = self.message_handle_store.generate();

        for entity in &entities {
            if !self.waiting_entities.contains_key(entity) {
                self.waiting_entities.insert(*entity, HashSet::new());
            }
            if let Some(message_set) = self.waiting_entities.get_mut(entity) {
                message_set.insert(new_handle);
            }
        }

        self.messages
            .insert(new_handle, (entities, channel, message));
    }

    pub fn add_entity(&mut self, entity: &E) {
        // put new entity into scope
        self.in_scope_entities.insert(*entity);

        // get a list of handles to messages ready to send
        let mut outgoing_message_handles = Vec::new();

        if let Some(message_set) = self.waiting_entities.get_mut(entity) {
            for message_handle in message_set.iter() {
                if let Some((entities, _, _)) = self.messages.get(message_handle) {
                    if entities
                        .iter()
                        .all(|entity| self.in_scope_entities.contains(entity))
                    {
                        outgoing_message_handles.push(*message_handle);
                    }
                }
            }
        }

        // get the messages ready to send, also clean up
        for outgoing_message_handle in outgoing_message_handles {
            let (entities, channel, message) =
                self.messages.remove(&outgoing_message_handle).unwrap();

            // push outgoing message
            self.ready_messages.push((channel, message));

            // recycle message handle
            self.message_handle_store
                .recycle_key(&outgoing_message_handle);

            // for all associated entities, remove from waitlist
            for entity in entities {
                let mut remove = false;
                if let Some(message_set) = self.waiting_entities.get_mut(&entity) {
                    message_set.remove(&outgoing_message_handle);
                    if message_set.is_empty() {
                        remove = true;
                    }
                }
                if remove {
                    self.waiting_entities.remove(&entity);
                }
            }
        }
    }

    pub fn remove_entity(&mut self, entity: &E) {
        self.in_scope_entities.remove(entity);
    }

    pub fn collect_ready_messages(&mut self, message_manager: &mut MessageManager<P, C>) {
        for (channel, message) in self.ready_messages.drain(..) {
            message_manager.send_message(channel, message);
        }
    }
}
