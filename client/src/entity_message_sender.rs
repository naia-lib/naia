use std::collections::{HashMap, VecDeque};

use crate::{constants::MESSAGE_HISTORY_SIZE, types::MsgId};
use naia_shared::{
    sequence_greater_than, sequence_less_than, PacketIndex, PacketNotifiable, Protocolize,
    ReplicateSafe, Tick,
};

pub struct EntityMessageSender<P: Protocolize> {
    // This SequenceBuffer is indexed by Tick
    outgoing_messages: OutgoingMessages<P>,
    // This SequenceBuffer is indexed by PacketIndex
    sent_messages: SentMessages,
    // Whether currently sending this tick
    send_locked: bool,
}

impl<P: Protocolize> EntityMessageSender<P> {
    pub fn new() -> Self {
        EntityMessageSender {
            outgoing_messages: OutgoingMessages::new(),
            sent_messages: SentMessages::new(),
            send_locked: false,
        }
    }

    pub fn send_entity_message<R: ReplicateSafe<P>>(
        &mut self,
        message: &R,
        client_tick: Tick,
    ) {
        let message_protocol = message.protocol_copy();

        self.outgoing_messages
            .push(client_tick, message_protocol);

        self.send_locked = false;
    }

    pub fn generate_outgoing_message_list(&mut self) -> VecDeque<(MsgId, Tick, P)> {
        if self.send_locked {
            panic!("Should not call this method when send_locked");
        }
        let mut outgoing_list = VecDeque::new();

        // Loop through outstanding messages and add them to the outgoing list
        let mut iter = self.outgoing_messages.iter();
        while let Some((tick, msg_map)) = iter.next() {
            msg_map.append_messages(&mut outgoing_list, *tick);
        }

        // if outgoing_list.len() > 0 {
        //     info!("appending {} messages", outgoing_list.len());
        // }

        self.send_locked = true;

        return outgoing_list;
    }

    pub fn message_written(&mut self, packet_index: PacketIndex, tick: Tick, message_id: MsgId) {
        self.sent_messages
            .push_front(packet_index, tick, message_id);
    }

    pub fn has_outgoing_messages(&self) -> bool {
        if self.send_locked {
            return false;
        }
        self.outgoing_messages.has_outgoing_messages()
    }

    pub fn on_tick(&mut self, server_receivable_tick: Tick) {
        self.send_locked = false;

        // Remove messages that would never be able to reach the Server
        self.outgoing_messages
            .pop_back_until_excluding(server_receivable_tick);
    }
}

impl<P: Protocolize> PacketNotifiable for EntityMessageSender<P> {
    fn notify_packet_delivered(&mut self, packet_index: PacketIndex) {
        if let Some(delivered_messages) = self.sent_messages.remove(packet_index) {
            for (tick, message_id) in delivered_messages.into_iter() {
                self.outgoing_messages.remove_message(tick, message_id);
            }
        }
    }

    fn notify_packet_dropped(&mut self, _dropped_packet_index: PacketIndex) {}
}

// SentMessages
struct SentMessages {
    // front big, back small
    // front recent, back past
    buffer: VecDeque<(PacketIndex, Vec<(Tick, MsgId)>)>,
}

impl SentMessages {
    pub fn new() -> Self {
        SentMessages {
            buffer: VecDeque::new(),
        }
    }

    pub fn push_front(&mut self, packet_index: PacketIndex, tick: Tick, msg_id: MsgId) {
        if let Some((old_packet_index, msg_list)) = self.buffer.front_mut() {
            if packet_index == *old_packet_index {
                // been here before, cool
                msg_list.push((tick, msg_id));
                return;
            }

            if sequence_less_than(packet_index, *old_packet_index) {
                panic!("this method should always receive increasing or equal packet indexes!")
            }
        } else {
            // nothing is in here
        }

        let mut msg_list = Vec::new();
        msg_list.push((tick, msg_id));
        self.buffer.push_front((packet_index, msg_list));

        // a good time to prune down this list
        while self.buffer.len() > MESSAGE_HISTORY_SIZE.into() {
            self.buffer.pop_back();
            //info!("pruning sent_messages buffer cause it got too big");
        }
    }

    pub fn remove(&mut self, packet_index: PacketIndex) -> Option<Vec<(Tick, MsgId)>> {
        let mut vec_index = self.buffer.len();

        // empty condition
        if vec_index == 0 {
            return None;
        }

        let mut found = false;

        loop {
            vec_index -= 1;

            if let Some((old_packet_index, _)) = self.buffer.get(vec_index) {
                if *old_packet_index == packet_index {
                    // found it!
                    found = true;
                } else {
                    // if old_packet_index is bigger than packet_index, give up, it's only getting
                    // bigger
                    if sequence_greater_than(*old_packet_index, packet_index) {
                        return None;
                    }
                }
            }

            if found {
                let (_, msg_list) = self.buffer.remove(vec_index).unwrap();
                //info!("found and removed packet: {}", packet_index);
                return Some(msg_list);
            }

            if vec_index == 0 {
                return None;
            }
        }
    }
}

// MessageMap
struct MessageMap<P: Protocolize> {
    map: HashMap<MsgId, P>,
    message_id: MsgId,
}

impl<P: Protocolize> MessageMap<P> {
    pub fn new() -> Self {
        MessageMap {
            map: HashMap::new(),
            message_id: 0,
        }
    }

    pub fn insert(&mut self, message: P) {
        let new_message_id = self.message_id;

        self.map.insert(new_message_id, message);

        self.message_id = self.message_id.wrapping_add(1);
    }

    pub fn append_messages(&self, list: &mut VecDeque<(MsgId, Tick, P)>, tick: Tick) {
        for (message_id, message) in &self.map {
            list.push_back((*message_id, tick, message.clone()));
        }
    }

    pub fn remove(&mut self, message_id: &MsgId) {
        self.map.remove(message_id);
    }

    pub fn len(&self) -> usize {
        return self.map.len();
    }
}

// OutgoingMessages
struct OutgoingMessages<P: Protocolize> {
    // front big, back small
    // front recent, back past
    buffer: VecDeque<(Tick, MessageMap<P>)>,
}

impl<P: Protocolize> OutgoingMessages<P> {
    pub fn new() -> Self {
        OutgoingMessages {
            buffer: VecDeque::new(),
        }
    }

    // should only push increasing ticks of messages
    pub fn push(&mut self, client_tick: Tick, message_protocol: P) {
        if let Some((front_tick, msg_map)) = self.buffer.front_mut() {
            if client_tick == *front_tick {
                // been here before, cool
                msg_map.insert(message_protocol);
                return;
            }

            if sequence_less_than(client_tick, *front_tick) {
                panic!("this method should always receive increasing or equal ticks!")
            }
        } else {
            // nothing is in here
        }

        let mut msg_map = MessageMap::new();
        msg_map.insert(message_protocol);
        self.buffer.push_front((client_tick, msg_map));

        // a good time to prune down this list
        while self.buffer.len() > MESSAGE_HISTORY_SIZE.into() {
            self.buffer.pop_back();
            //info!("pruning outgoing_messages buffer cause it got too big");
        }
    }

    pub fn pop_back_until_excluding(&mut self, until_tick: Tick) {
        loop {
            if let Some((old_tick, _)) = self.buffer.back() {
                if sequence_less_than(until_tick, *old_tick) {
                    return;
                }
            } else {
                return;
            }

            self.buffer.pop_back();
        }
    }

    pub fn iter(&self) -> impl Iterator<Item = &(Tick, MessageMap<P>)> {
        self.buffer.iter().rev()
    }

    pub fn remove_message(&mut self, tick: Tick, msg_id: MsgId) {
        let mut index = self.buffer.len();

        if index == 0 {
            // empty condition
            return;
        }

        loop {
            index -= 1;

            let mut remove = false;

            if let Some((old_tick, msg_map)) = self.buffer.get_mut(index) {
                if *old_tick == tick {
                    // found it!
                    msg_map.remove(&msg_id);
                    //info!("removed delivered message! tick: {}, msg_id: {}",
                    // tick, msg_id);
                    if msg_map.len() == 0 {
                        remove = true;
                    }
                } else {
                    // if tick is less than old tick, no sense continuing, only going to get bigger
                    // as we go
                    if sequence_greater_than(*old_tick, tick) {
                        return;
                    }
                }
            }

            if remove {
                self.buffer.remove(index);
            }

            if index == 0 {
                return;
            }
        }
    }

    pub fn has_outgoing_messages(&self) -> bool {
        !self.buffer.is_empty()
    }
}