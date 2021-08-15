use std::{
    collections::{HashMap, VecDeque},
    rc::Rc,
    vec::Vec,
};

use crate::{
    replicate::{replicate::Replicate, protocol_type::ProtocolType, replicate::MessageClone},
    manifest::Manifest,
    PacketReader,
};

/// Handles incoming/outgoing messages, tracks the delivery status of Messages so
/// that guaranteed Messages can be re-transmitted to the remote host
#[derive(Debug)]
pub struct MessageManager<T: ProtocolType> {
    queued_outgoing_messages: VecDeque<(bool, Rc<Box<dyn Replicate<T>>>)>,
    queued_incoming_messages: VecDeque<T>,
    sent_guaranteed_messages: HashMap<u16, Vec<Rc<Box<dyn Replicate<T>>>>>,
    last_popped_message_guarantee: bool,
}

impl<T: ProtocolType> MessageManager<T> {
    /// Creates a new MessageManager
    pub fn new() -> Self {
        MessageManager {
            queued_outgoing_messages: VecDeque::new(),
            queued_incoming_messages: VecDeque::new(),
            sent_guaranteed_messages: HashMap::new(),
            last_popped_message_guarantee: false,
        }
    }

    /// Occurs when a packet has been notified as delivered. Stops tracking the
    /// status of Messages in that packet.
    pub fn notify_packet_delivered(&mut self, packet_index: u16) {
        self.sent_guaranteed_messages.remove(&packet_index);
    }

    /// Occurs when a packet has been notified as having been dropped. Queues up
    /// any guaranteed Messages that were lost in the packet for retransmission.
    pub fn notify_packet_dropped(&mut self, packet_index: u16) {
        if let Some(dropped_messages_list) = self.sent_guaranteed_messages.get(&packet_index) {
            for dropped_message in dropped_messages_list.into_iter() {
                self.queued_outgoing_messages.push_back((true, dropped_message.clone()));
            }

            self.sent_guaranteed_messages.remove(&packet_index);
        }
    }

    /// Returns whether the Manager has queued Messages that can be transmitted to
    /// the remote host
    pub fn has_outgoing_messages(&self) -> bool {
        return self.queued_outgoing_messages.len() != 0;
    }

    /// Gets the next queued Message to be transmitted
    pub fn pop_outgoing_message(&mut self, packet_index: u16) -> Option<Rc<Box<dyn Replicate<T>>>> {
        match self.queued_outgoing_messages.pop_front() {
            Some((guaranteed, message)) => {
                //place in transmission record if this is a gauranteed message
                if guaranteed {
                    if !self.sent_guaranteed_messages.contains_key(&packet_index) {
                        let sent_messages_list: Vec<Rc<Box<dyn Replicate<T>>>> = Vec::new();
                        self.sent_guaranteed_messages.insert(packet_index, sent_messages_list);
                    }

                    if let Some(sent_messages_list) = self.sent_guaranteed_messages.get_mut(&packet_index) {
                        sent_messages_list.push(message.clone());
                    }
                }

                self.last_popped_message_guarantee = guaranteed;

                Some(message)
            }
            None => None,
        }
    }

    /// If  the last popped Message from the queue somehow wasn't able to be
    /// written into a packet, put the Message back into the front of the queue
    pub fn unpop_outgoing_message(&mut self, packet_index: u16, message: &Rc<Box<dyn Replicate<T>>>) {
        let cloned_message = message.clone();

        if self.last_popped_message_guarantee {
            if let Some(sent_messages_list) = self.sent_guaranteed_messages.get_mut(&packet_index) {
                sent_messages_list.pop();
                if sent_messages_list.len() == 0 {
                    self.sent_guaranteed_messages.remove(&packet_index);
                }
            }
        }

        self.queued_outgoing_messages.push_front((self.last_popped_message_guarantee, cloned_message));
    }

    /// Queues an Message to be transmitted to the remote host
    pub fn queue_outgoing_message(&mut self, message: &impl Replicate<T>, guaranteed_delivery: bool) {
        let clone = Rc::new(MessageClone::clone_box(message));
        self.queued_outgoing_messages.push_back((guaranteed_delivery, clone));
    }

    /// Returns whether any Messages have been received that must be handed to the
    /// application
    pub fn has_incoming_messages(&self) -> bool {
        return self.queued_incoming_messages.len() != 0;
    }

    /// Get the most recently received Message
    pub fn pop_incoming_message(&mut self) -> Option<T> {
        return self.queued_incoming_messages.pop_front();
    }

    /// Given incoming packet data, read transmitted Messages and store them to be
    /// returned to the application
    pub fn process_data(
        &mut self,
        reader: &mut PacketReader,
        manifest: &Manifest<T>)
    {
        let message_count = reader.read_u8();
        for _x in 0..message_count {
            let naia_id: u16 = reader.read_u16();

            let new_message = manifest.create_replicate(naia_id, reader);
            self.queued_incoming_messages.push_back(new_message);
        }
    }
}
