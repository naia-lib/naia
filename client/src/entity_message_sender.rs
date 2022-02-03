use std::{collections::VecDeque, hash::Hash};

use naia_shared::{Protocolize, ReplicateSafe, SequenceBuffer};

const MESSAGE_HISTORY_SIZE: u16 = 64;

pub struct EntityMessageSender<P: Protocolize, E: Copy + Eq + Hash> {
    outgoing_messages: SequenceBuffer<Vec<(E, P)>>,
}

impl<P: Protocolize, E: Copy + Eq + Hash> EntityMessageSender<P, E> {
    pub fn new() -> Self {
        EntityMessageSender {
            outgoing_messages: SequenceBuffer::with_capacity(MESSAGE_HISTORY_SIZE),
        }
    }

    pub fn send_entity_message<R: ReplicateSafe<P>>(&mut self, entity: &E, message: &R, client_tick: u16) {
        let message_protocol = message.protocol_copy();

        if !self.outgoing_messages.exists(client_tick) {
            self.outgoing_messages
                .insert(client_tick, Vec::new());
        }
        let list = self.outgoing_messages.get_mut(client_tick).unwrap();
        list.push((*entity, message_protocol));
    }

    pub fn get_messages(&mut self, server_receivable_tick: u16) -> VecDeque<(u16, E, P)> {
        let mut outgoing_list = VecDeque::new();

        // Remove messages that would never be able to reach the Server
        self.outgoing_messages.remove_until(server_receivable_tick);

        // Loop through outstanding messages and add them to the outgoing list
        let current_tick = self.outgoing_messages.newest();
        for tick in server_receivable_tick..=current_tick {
            if let Some(message_list) = self.outgoing_messages.get_mut(tick) {
                for (entity, message) in message_list {
                    outgoing_list.push_back((tick, *entity, message.clone()));
                }
            }
        }
        return outgoing_list;
    }
}