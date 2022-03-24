use naia_serde::derive_serde;

use crate::{connection::packet_type::PacketType, serde, types::PacketIndex};

// This header provides reliability information.
#[derive(Copy, Debug)]
#[derive_serde]
pub struct StandardHeader {
    packet_type: PacketType,
    // Packet index identifying this packet
    sender_packet_index: PacketIndex,
    // This is the last acknowledged sequence number.
    last_recv_packet_index: PacketIndex,
    // This is an bitfield of all last 32 acknowledged packages
    ack_field: u32,
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
        last_recv_packet_index: PacketIndex,
        ack_field: u32,
    ) -> StandardHeader {
        StandardHeader {
            packet_type,
            sender_packet_index,
            last_recv_packet_index,
            ack_field,
        }
    }

    /// Returns the packet type indicated by the header
    pub fn packet_type(&self) -> PacketType {
        self.packet_type
    }

    /// Returns the sequence number from this packet.
    pub fn sender_packet_index(&self) -> u16 {
        self.sender_packet_index
    }

    /// Returns bit field of all last 32 acknowledged packages.
    pub fn sender_ack_bitfield(&self) -> u32 {
        self.ack_field
    }

    /// Returns last acknowledged sequence number.
    pub fn sender_ack_index(&self) -> u16 {
        self.last_recv_packet_index
    }
}
