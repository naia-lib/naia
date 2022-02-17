use std::{
    collections::{HashMap, VecDeque},
    vec::Vec,
};

use naia_socket_shared::PacketReader;

use super::{
    manifest::Manifest,
    packet_notifiable::PacketNotifiable,
    protocolize::{ProtocolKindType, Protocolize},
    replicate::ReplicateSafe,
};

/// Handles incoming/outgoing messages, tracks the delivery status of Messages
/// so that guaranteed Messages can be re-transmitted to the remote host
#[derive(Debug)]
pub struct MessageManager<P: Protocolize> {
    queued_outgoing_messages: VecDeque<(bool, P)>,
    queued_incoming_messages: VecDeque<P>,
    sent_guaranteed_messages: HashMap<u16, Vec<P>>,
    last_popped_message_guarantee: bool,
}

impl<P: Protocolize> MessageManager<P> {
    /// Creates a new MessageManager
    pub fn new() -> Self {
        MessageManager {
            queued_outgoing_messages: VecDeque::new(),
            queued_incoming_messages: VecDeque::new(),
            sent_guaranteed_messages: HashMap::new(),
            last_popped_message_guarantee: false,
        }
    }

    /// Returns whether the Manager has queued Messages that can be transmitted
    /// to the remote host
    pub fn has_outgoing_messages(&self) -> bool {
        return self.queued_outgoing_messages.len() != 0;
    }

    /// Gets the next queued Message to be transmitted
    pub fn peek_outgoing_message(&self) -> Option<&P> {
        match self.queued_outgoing_messages.front() {
            Some((_, message)) => Some(message),
            None => None,
        }
    }

    /// Gets the next queued Message to be transmitted
    pub fn pop_outgoing_message(&mut self, packet_index: u16) -> Option<P> {
        match self.queued_outgoing_messages.pop_front() {
            Some((guaranteed, message)) => {
                //place in transmission record if this is a guaranteed message
                if guaranteed {
                    if !self.sent_guaranteed_messages.contains_key(&packet_index) {
                        let sent_messages_list: Vec<P> = Vec::new();
                        self.sent_guaranteed_messages
                            .insert(packet_index, sent_messages_list);
                    }

                    if let Some(sent_messages_list) =
                        self.sent_guaranteed_messages.get_mut(&packet_index)
                    {
                        sent_messages_list.push(message.clone());
                    }
                }

                self.last_popped_message_guarantee = guaranteed;

                Some(message)
            }
            None => None,
        }
    }

    /// Queues an Message to be transmitted to the remote host
    pub fn queue_outgoing_message<R: ReplicateSafe<P>>(
        &mut self,
        message: &R,
        guaranteed_delivery: bool,
    ) {
        self.queued_outgoing_messages
            .push_back((guaranteed_delivery, message.protocol_copy()));
    }

    /// Returns whether any Messages have been received that must be handed to
    /// the application
    pub fn has_incoming_messages(&self) -> bool {
        return self.queued_incoming_messages.len() != 0;
    }

    /// Get the most recently received Message
    pub fn pop_incoming_message(&mut self) -> Option<P> {
        return self.queued_incoming_messages.pop_front();
    }

    /// Given incoming packet data, read transmitted Messages and store them to
    /// be returned to the application
    pub fn process_data(&mut self, reader: &mut PacketReader, manifest: &Manifest<P>) {
        let message_count = reader.read_u8();
        for _x in 0..message_count {
            let component_kind: P::Kind = P::Kind::from_u16(reader.read_u16());

            let new_message = manifest.create_replica(component_kind, reader);
            self.queued_incoming_messages.push_back(new_message);
        }
    }
}

impl<P: Protocolize> PacketNotifiable for MessageManager<P> {
    /// Occurs when a packet has been notified as delivered. Stops tracking the
    /// status of Messages in that packet.
    fn notify_packet_delivered(&mut self, packet_index: u16) {
        self.sent_guaranteed_messages.remove(&packet_index);
    }

    /// Occurs when a packet has been notified as having been dropped. Queues up
    /// any guaranteed Messages that were lost in the packet for retransmission.
    fn notify_packet_dropped(&mut self, packet_index: u16) {
        if let Some(dropped_messages_list) = self.sent_guaranteed_messages.get(&packet_index) {
            for dropped_message in dropped_messages_list.into_iter() {
                self.queued_outgoing_messages
                    .push_back((true, dropped_message.clone()));
            }

            self.sent_guaranteed_messages.remove(&packet_index);
        }
    }
}
