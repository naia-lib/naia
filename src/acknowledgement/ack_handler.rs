use std::collections::HashMap;

use crate::sequence_buffer::{sequence_greater_than, sequence_less_than, SequenceNumber, SequenceBuffer};

const REDUNDANT_PACKET_ACKS_SIZE: u16 = 32;
const DEFAULT_SEND_PACKETS_SIZE: usize = 256;

/// Responsible for handling the acknowledgment of packets.
pub struct AckHandler {
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

impl AckHandler {
    /// Constructs a new `AckHandler` with which you can perform acknowledgment operations.
    pub fn new() -> Self {
        AckHandler {
            sequence_number: 0,
            remote_ack_sequence_num: u16::max_value(),
            sent_packets: HashMap::with_capacity(DEFAULT_SEND_PACKETS_SIZE),
            received_packets: SequenceBuffer::with_capacity(REDUNDANT_PACKET_ACKS_SIZE + 1),
        }
    }

    /// Returns the current number of not yet acknowledged packets
    pub fn packets_in_flight(&self) -> u16 {
        self.sent_packets.len() as u16
    }

    /// Returns the next sequence number to send.
    pub fn local_sequence_num(&self) -> SequenceNumber {
        self.sequence_number
    }

    /// Returns the last sequence number received from the remote host (+1)
    pub fn remote_sequence_num(&self) -> SequenceNumber {
        self.received_packets.sequence_num().wrapping_sub(1)
    }

    /// Returns the `ack_bitfield` corresponding to which of the past 32 packets we've
    /// successfully received.
    pub fn ack_bitfield(&self) -> u32 {
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

    /// Process the incoming sequence number.
    ///
    /// - Acknowledge the incoming sequence number
    /// - Update dropped packets
    pub fn process_incoming(
        &mut self,
        message: String,
    ) -> String {
        ///TODO: Get these values off the header of the packet
        //        remote_seq_num: u16,
        //        remote_ack_seq: u16,
        //        mut remote_ack_field: u32,


        // ensure that `self.remote_ack_sequence_num` is always increasing (with wrapping)
        if sequence_greater_than(remote_ack_seq, self.remote_ack_sequence_num) {
            self.remote_ack_sequence_num = remote_ack_seq;
        }

        self.received_packets
            .insert(remote_seq_num, ReceivedPacket {});

        // the current `remote_ack_seq` was (clearly) received so we should remove it
        self.sent_packets.remove(&remote_ack_seq);

        // The `remote_ack_field` is going to include whether or not the past 32 packets have been
        // received successfully. If so, we have no need to resend old packets.
        for i in 1..=REDUNDANT_PACKET_ACKS_SIZE {
            let ack_sequence = remote_ack_seq.wrapping_sub(i);
            if remote_ack_field & 1 == 1 {
                self.sent_packets.remove(&ack_sequence);
            }
            remote_ack_field >>= 1;
        }

        ///TODO: Make sure you've stripped the ack header off before returning this

        message
    }

    /// Enqueues the outgoing packet for acknowledgment.
    pub fn process_outgoing(
        &mut self,
        message: String,
    ) -> String {
        self.sent_packets.insert(
            self.sequence_number,
            SentPacket {},
        );

        // bump the local sequence number for the next outgoing packet
        self.sequence_number = self.sequence_number.wrapping_add(1);

        ///TODO: Add Ack Header onto message!
        //        let seq_num = self.local_sequence_num();
        //        let last_seq = self.remote_sequence_num();
        //        let bit_field = self.ack_bitfield();

        message
    }

    /// Returns a `Vec` of packets we believe have been dropped.
    pub fn dropped_packets(&mut self) -> Vec<SentPacket> {
        let mut sent_sequences: Vec<SequenceNumber> = self.sent_packets.keys().cloned().collect();
        sent_sequences.sort();

        let remote_ack_sequence = self.remote_ack_sequence_num;
        sent_sequences
            .into_iter()
            .filter(|s| {
                if sequence_less_than(*s, remote_ack_sequence) {
                    remote_ack_sequence.wrapping_sub(*s) > REDUNDANT_PACKET_ACKS_SIZE
                } else {
                    false
                }
            })
            .flat_map(|s| self.sent_packets.remove(&s))
            .collect()
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct SentPacket;

#[derive(Clone, Default)]
pub struct ReceivedPacket;