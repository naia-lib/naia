use std::{
    collections::{HashMap, VecDeque},
    vec::Vec,
};

use crate::PacketWriteState;
use naia_socket_shared::PacketReader;

use super::{
    manifest::Manifest,
    message_packet_writer::MessagePacketWriter,
    packet_notifiable::PacketNotifiable,
    protocolize::{ProtocolKindType, Protocolize},
    replicate::ReplicateSafe,
};

/// Handles incoming/outgoing messages, tracks the delivery status of Messages
/// so that guaranteed Messages can be re-transmitted to the remote host
pub struct MessageManager<P: Protocolize> {
    queued_outgoing_messages: VecDeque<(bool, P)>,
    queued_incoming_messages: VecDeque<P>,
    sent_guaranteed_messages: HashMap<u16, Vec<P>>,
    message_writer: MessagePacketWriter,
}

impl<P: Protocolize> MessageManager<P> {
    /// Creates a new MessageManager
    pub fn new() -> Self {
        MessageManager {
            queued_outgoing_messages: VecDeque::new(),
            queued_incoming_messages: VecDeque::new(),
            sent_guaranteed_messages: HashMap::new(),
            message_writer: MessagePacketWriter::new(),
        }
    }

    // Outgoing Messages

    /// Returns whether the Manager has queued Messages that can be transmitted
    /// to the remote host
    pub fn has_outgoing_messages(&self) -> bool {
        return !self.queued_outgoing_messages.is_empty();
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

                Some(message)
            }
            None => None,
        }
    }

    /// Queues an Message to be transmitted to the remote host
    pub fn send_message<R: ReplicateSafe<P>>(&mut self, message: &R, guaranteed_delivery: bool) {
        self.queued_outgoing_messages
            .push_back((guaranteed_delivery, message.protocol_copy()));
    }

    // Incoming Messages

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
    pub fn process_message_data(&mut self, reader: &mut PacketReader, manifest: &Manifest<P>) {
        let message_count = reader.read_u8();
        for _x in 0..message_count {
            let component_kind: P::Kind = P::Kind::from_u16(reader.read_u16());

            let new_message = manifest.create_replica(component_kind, reader);
            self.queued_incoming_messages.push_back(new_message);
        }
    }

    // MessageWriter

    /// Write into outgoing packet
    pub fn queue_writes(&mut self, write_state: &mut PacketWriteState) {
        loop {
            if let Some((_, peeked_message)) = self.queued_outgoing_messages.front() {
                if !self
                    .message_writer
                    .message_fits(write_state, peeked_message)
                {
                    break;
                }
            } else {
                break;
            }

            let popped_message = self.pop_outgoing_message(write_state.packet_index).unwrap();
            self.message_writer
                .queue_write(write_state, &popped_message);
        }
    }

    pub fn flush_writes(&mut self, out_bytes: &mut Vec<u8>) {
        self.message_writer.flush_writes(out_bytes);
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
