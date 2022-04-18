use naia_serde::derive_serde;

use crate::{connection::packet_type::PacketType, serde, types::PacketIndex};

// This header provides reliability information.
#[derive(Copy, Debug)]
#[derive_serde]
pub struct StandardHeader {
    pub packet_type: PacketType,
    // Packet index identifying this packet
    pub sender_packet_index: PacketIndex,
    // This is the last acknowledged sequence number.
    pub sender_ack_index: PacketIndex,
    // This is an bitfield of all last 32 acknowledged packages
    pub sender_ack_bitfield: u32,
}

impl StandardHeader {
    /// When we compose packet headers, the local sequence becomes the sequence
    /// number of the packet, and the remote sequence becomes the ack.
    /// The ack bitfield is calculated by looking into a queue of up to 33
    /// packets, containing sequence numbers in the range [remote sequence - 32,
    /// remote sequence]. We set bit n (in [1,32]) in ack bits to 1 if the
    /// sequence number remote sequence - n is in the received queue.
    pub fn new(
        packet_type: PacketType,
        sender_packet_index: PacketIndex,
        sender_ack_index: PacketIndex,
        sender_ack_bitfield: u32,
    ) -> StandardHeader {
        StandardHeader {
            packet_type,
            sender_packet_index,
            sender_ack_index,
            sender_ack_bitfield,
        }
    }
}
