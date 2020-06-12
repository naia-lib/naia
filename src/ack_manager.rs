use std::collections::HashMap;

use log::{info};

use super::{
    standard_header::StandardHeader,
    sequence_buffer::{sequence_greater_than, SequenceNumber, SequenceBuffer},
};

use crate::{PacketType, EventType, EntityType, HostType, EventManager, ServerEntityManager};

const REDUNDANT_PACKET_ACKS_SIZE: u16 = 32;
const DEFAULT_SEND_PACKETS_SIZE: usize = 256;

pub struct AckManager {
    host_type: HostType,
    // Local sequence number which we'll bump each time we send a new packet over the network.
    sequence_number: SequenceNumber,
    // The last acked sequence number of the packets we've sent to the remote host.
    remote_ack_sequence_num: SequenceNumber,
    // Using a `Hashmap` to track every packet we send out so we can ensure that we can resend when
    // dropped.
    sent_packets: HashMap<u16, SentPacket>,
    // However, we can only reasonably ack up to `REDUNDANT_PACKET_ACKS_SIZE + 1` packets on each
    // message we send so this should be that large.
    received_packets: SequenceBuffer<ReceivedPacket>,
}

impl AckManager {
    pub fn new(host_type: HostType) -> Self {
        AckManager {
            host_type,
            sequence_number: 0,
            remote_ack_sequence_num: u16::max_value(),
            sent_packets: HashMap::with_capacity(DEFAULT_SEND_PACKETS_SIZE),
            received_packets: SequenceBuffer::with_capacity(REDUNDANT_PACKET_ACKS_SIZE + 1),
        }
    }

    pub fn local_sequence_num(&self) -> SequenceNumber {
        self.sequence_number
    }

    pub fn process_incoming<T: EventType, U: EntityType>(&mut self,
                                                         event_manager: &mut EventManager<T>,
                                                         entity_manager: &mut Option<&mut ServerEntityManager<U>>,
                                                         payload: &[u8]) -> Box<[u8]> {
        let (header, stripped_message) = StandardHeader::read(payload);
        let remote_seq_num = header.sequence();
        let remote_ack_seq = header.ack_seq();
        let mut remote_ack_field = header.ack_field();

        self.received_packets
            .insert(remote_seq_num, ReceivedPacket {});

        // ensure that `self.remote_ack_sequence_num` is always increasing (with wrapping)
        if sequence_greater_than(remote_ack_seq, self.remote_ack_sequence_num) {
            self.remote_ack_sequence_num = remote_ack_seq;
        }

        // the current `remote_ack_seq` was (clearly) received so we should remove it
        if let Some(sent_packet) = self.sent_packets.get(&remote_ack_seq) {
            if sent_packet.packet_type == PacketType::Data {
                self.notify_packet_delivered(event_manager, entity_manager, remote_ack_seq);
            }

            self.sent_packets.remove(&remote_ack_seq);
        }

        // The `remote_ack_field` is going to include whether or not the past 32 packets have been
        // received successfully. If so, we have no need to resend old packets.
        for i in 1..=REDUNDANT_PACKET_ACKS_SIZE {
            let ack_sequence = remote_ack_seq.wrapping_sub(i);
            if let Some(sent_packet) = self.sent_packets.get(&ack_sequence) {
                if remote_ack_field & 1 == 1 {
                    if sent_packet.packet_type == PacketType::Data {
                        self.notify_packet_delivered(event_manager, entity_manager, ack_sequence);
                    }

                    self.sent_packets.remove(&ack_sequence);
                } else {
                    if sent_packet.packet_type == PacketType::Data {
                        self.notify_packet_dropped(event_manager, entity_manager, ack_sequence);
                    }
                    self.sent_packets.remove(&ack_sequence);
                }
            }

            remote_ack_field >>= 1;
        }

        stripped_message
    }

    pub fn process_outgoing(&mut self, packet_type: PacketType, payload: &[u8]) -> Box<[u8]> {

        // Add Ack Header onto message!
        let mut header_bytes = Vec::new();

        let seq_num = self.local_sequence_num();
        let last_seq = self.remote_sequence_num();
        let bit_field = self.ack_bitfield();

        let header = StandardHeader::new(packet_type, seq_num, last_seq, bit_field);
        header.write(&mut header_bytes);

        // Ack stuff //
        self.sent_packets.insert(
            self.sequence_number,
            SentPacket {
                id: self.sequence_number as u32,
                packet_type,
            },
        );

        // bump the local sequence number for the next outgoing packet
        self.sequence_number = self.sequence_number.wrapping_add(1);
        ///////////////

        [header_bytes.as_slice(), &payload]
            .concat()
            .into_boxed_slice()
    }

    fn notify_packet_delivered<T: EventType, U: EntityType>(&self, event_manager: &mut EventManager<T>,
                                                            entity_manager: &mut Option<&mut ServerEntityManager<U>>,
                                                            packet_sequence_number: u16) {
        let host_type_string = match self.host_type {
            HostType::Server => "Server",
            HostType::Client => "Client",
        };
        info!("-------------- notify -- [{} Packet ({})] -- DELIVERED! --------------", host_type_string, packet_sequence_number);
        event_manager.notify_packet_delivered(packet_sequence_number);
        if let Some(server_entity_manager) = entity_manager {
            server_entity_manager.notify_packet_delivered(packet_sequence_number);
        }
    }

    fn notify_packet_dropped<T: EventType, U: EntityType>(&self, event_manager: &mut EventManager<T>,
                                                          entity_manager: &mut Option<&mut ServerEntityManager<U>>,
                                                          packet_sequence_number: u16) {
        let host_type_string = match self.host_type {
            HostType::Server => "Server",
            HostType::Client => "Client",
        };
        info!("---XXXXXXXX--- notify -- [{} Packet ({})] -- DROPPED! ---XXXXXXXX---", host_type_string, packet_sequence_number);
        event_manager.notify_packet_dropped(packet_sequence_number);
        if let Some(server_entity_manager) = entity_manager {
            server_entity_manager.notify_packet_dropped(packet_sequence_number);
        }
    }

    fn remote_sequence_num(&self) -> SequenceNumber {
        self.received_packets.sequence_num().wrapping_sub(1)
    }

    fn ack_bitfield(&self) -> u32 {
        let most_recent_remote_seq_num: u16 = self.remote_sequence_num();
        let mut ack_bitfield: u32 = 0;
        let mut mask: u32 = 1;

        // iterate the past `REDUNDANT_PACKET_ACKS_SIZE` received packets and set the corresponding
        // bit for each packet which exists in the buffer.
        for i in 1..=REDUNDANT_PACKET_ACKS_SIZE {
            let sequence = most_recent_remote_seq_num.wrapping_sub(i);
            if self.received_packets.exists(sequence) {
                ack_bitfield |= mask;
            }
            mask <<= 1;
        }

        ack_bitfield
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct SentPacket {
    pub id: u32,
    pub packet_type: PacketType,
}

#[derive(Clone, Default)]
pub struct ReceivedPacket;