use std::{
    collections::{HashMap, VecDeque},
    hash::Hash,
};

use naia_shared::{PacketNotifiable, Protocolize, ReplicateSafe, SequenceBuffer};

const MESSAGE_HISTORY_SIZE: u16 = 64;

pub type MsgId = u8;
type PacketIndex = u16;
pub type Tick = u16;

pub struct EntityMessageSender<P: Protocolize, E: Copy + Eq + Hash> {
    // This SequenceBuffer is indexed by Tick
    outgoing_messages: SequenceBuffer<MessageMap<P, E>>,
    // This SequenceBuffer is indexed by PacketIndex
    sent_messages: SequenceBuffer<Vec<(Tick, MsgId)>>,
}

impl<P: Protocolize, E: Copy + Eq + Hash> EntityMessageSender<P, E> {
    pub fn new() -> Self {
        EntityMessageSender {
            outgoing_messages: SequenceBuffer::with_capacity(MESSAGE_HISTORY_SIZE),
            sent_messages: SequenceBuffer::with_capacity(MESSAGE_HISTORY_SIZE),
        }
    }

    pub fn send_entity_message<R: ReplicateSafe<P>>(
        &mut self,
        entity: &E,
        message: &R,
        client_tick: Tick,
    ) {
        let message_protocol = message.protocol_copy();

        if !self.outgoing_messages.exists(client_tick) {
            self.outgoing_messages
                .insert(client_tick, MessageMap::new());
        }
        if let Some(message_map) = self.outgoing_messages.get_mut(client_tick) {
            message_map.insert(*entity, message_protocol);
        }
    }

    pub fn messages(&mut self, server_receivable_tick: Tick) -> VecDeque<(MsgId, Tick, E, P)> {
        let mut outgoing_list = VecDeque::new();

        // Remove messages that would never be able to reach the Server
        self.outgoing_messages.remove_until(server_receivable_tick);

        // Loop through outstanding messages and add them to the outgoing list
        let mut index_tick = server_receivable_tick;
        let current_tick = self.outgoing_messages.newest();

        loop {

            if let Some(message_list) = self.outgoing_messages.get_mut(index_tick) {
                message_list.append_messages(&mut outgoing_list, index_tick);
            }

            if index_tick == current_tick {
                break;
            }

            index_tick = index_tick.wrapping_add(1);
        }

        return outgoing_list;
    }

    pub fn message_written(&mut self, packet_index: PacketIndex, tick: Tick, message_id: MsgId) {
        if !self.sent_messages.exists(packet_index) {
            self.sent_messages.insert(packet_index, Vec::new());
        }
        if let Some(list) = self.sent_messages.get_mut(packet_index) {
            list.push((tick, message_id));
        }
    }
}

impl<P: Protocolize, E: Copy + Eq + Hash> PacketNotifiable for EntityMessageSender<P, E> {
    fn notify_packet_delivered(&mut self, packet_index: PacketIndex) {
        if let Some(delivered_messages) = self.sent_messages.remove(packet_index) {
            for (tick, message_id) in delivered_messages.into_iter() {
                if let Some(message_map) = self.outgoing_messages.get_mut(tick) {
                    message_map.remove(&message_id);
                }
            }
        }
    }

    fn notify_packet_dropped(&mut self, _dropped_packet_index: u16) {}
}

// MessageMap

struct MessageMap<P: Protocolize, E: Copy + Eq + Hash> {
    map: HashMap<MsgId, (E, P)>,
    message_id: MsgId,
}

impl<P: Protocolize, E: Copy + Eq + Hash> MessageMap<P, E> {
    pub fn new() -> Self {
        MessageMap {
            map: HashMap::new(),
            message_id: 0,
        }
    }

    pub fn insert(&mut self, entity: E, message: P) {
        let new_message_id = self.message_id;

        self.map.insert(new_message_id, (entity, message));

        self.message_id += 1;
    }

    pub fn append_messages(&self, list: &mut VecDeque<(MsgId, Tick, E, P)>, tick: Tick) {
        for (message_id, (entity, message)) in &self.map {
            list.push_back((*message_id, tick, *entity, message.clone()));
        }
    }

    pub fn remove(&mut self, message_id: &MsgId) {
        self.map.remove(message_id);
    }
}
