use std::{
    collections::{HashMap, VecDeque},
    vec::Vec,
};

use crate::{constants::MTU_SIZE_BITS, ManagerType, PacketIndex};
use naia_serde::{BitCounter, BitReader, BitWrite, BitWriter, Serde};

use super::{
    manifest::Manifest, packet_notifiable::PacketNotifiable, protocolize::Protocolize,
    replicate::ReplicateSafe,
};

/// Handles incoming/outgoing messages, tracks the delivery status of Messages
/// so that guaranteed Messages can be re-transmitted to the remote host
pub struct MessageManager<P: Protocolize> {
    queued_outgoing_messages: VecDeque<(bool, P)>,
    queued_incoming_messages: VecDeque<P>,
    sent_guaranteed_messages: HashMap<u16, Vec<P>>,
}

impl<P: Protocolize> MessageManager<P> {
    /// Creates a new MessageManager
    pub fn new() -> Self {
        MessageManager {
            queued_outgoing_messages: VecDeque::new(),
            queued_incoming_messages: VecDeque::new(),
            sent_guaranteed_messages: HashMap::new(),
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

    /// Get the most recently received Message
    pub fn pop_incoming_message(&mut self) -> Option<P> {
        return self.queued_incoming_messages.pop_front();
    }

    /// Given incoming packet data, read transmitted Messages and store them to
    /// be returned to the application
    pub fn process_message_data(&mut self, reader: &mut BitReader, manifest: &Manifest<P>) {
        let message_count = u8::de(reader).unwrap();
        for _x in 0..message_count {
            let component_kind: P::Kind = P::Kind::de(reader).unwrap();

            let new_message = manifest.create_replica(component_kind, reader);
            self.queued_incoming_messages.push_back(new_message);
        }
    }

    // MessageWriter

    /// Write into outgoing packet
    pub fn write_messages(&mut self, writer: &mut BitWriter, packet_index: PacketIndex) {
        let mut message_count = 0;

        // Header
        {
            // Measure
            let current_packet_size = writer.bit_count();
            if current_packet_size > MTU_SIZE_BITS {
                return;
            }

            let mut counter = BitCounter::new();
            MessageManager::<P>::write_header(&mut counter, 123);

            // Check for overflow
            if current_packet_size + counter.bit_count() > MTU_SIZE_BITS {
                return;
            }

            // Find how many messages will fit into the packet
            for (_, message) in self.queued_outgoing_messages.iter() {
                MessageManager::<P>::write_message(&mut counter, message);
                if current_packet_size + counter.bit_count() <= MTU_SIZE_BITS {
                    message_count += 1;
                    if message_count == u8::MAX {
                        break;
                    }
                } else {
                    break;
                }
            }

            // If no messages will fit, abort
            if message_count == 0 {
                return;
            }

            // Write header
            MessageManager::<P>::write_header(writer, message_count);
        }

        // Messages
        {
            for _ in 0..message_count {
                // Pop message
                let popped_message = self.pop_outgoing_message(packet_index).unwrap();

                // Write message
                MessageManager::<P>::write_message(writer, &popped_message);
            }
        }
    }

    /// Write bytes into an outgoing packet
    pub fn write_header<S: BitWrite>(writer: &mut S, message_count: u8) {
        //Write manager "header" (manager type & message count)

        // write manager type
        ManagerType::Message.ser(writer);

        // write number of messages
        message_count.ser(writer);
    }

    /// Writes an Message into the Writer's internal buffer, which will
    /// eventually be put into the outgoing packet
    pub fn write_message<S: BitWrite>(writer: &mut S, message: &P) {
        // write message kind
        message.dyn_ref().kind().ser(writer);

        // write payload
        message.write(writer);
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
