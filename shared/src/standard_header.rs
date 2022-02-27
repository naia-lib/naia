use crate::{derive_serde, serde};

use crate::packet_type::PacketType;

// This header provides reliability information.
#[derive(Copy, Debug)]
#[derive_serde]
pub struct StandardHeader {
    p_type: PacketType,
    // This is the sequence number so that we can know where in the sequence of packages this
    // packet belongs.
    local_packet_index: u16,
    // This is the last acknowledged sequence number.
    last_remote_packet_index: u16,
    // This is an bitfield of all last 32 acknowledged packages
    ack_field: u32,
    // This the the current Tick of the host,
    host_tick: u16,
}

impl StandardHeader {
    /// When we compose packet headers, the local sequence becomes the sequence
    /// number of the packet, and the remote sequence becomes the ack.
    /// The ack bitfield is calculated by looking into a queue of up to 33
    /// packets, containing sequence numbers in the range [remote sequence - 32,
    /// remote sequence]. We set bit n (in [1,32]) in ack bits to 1 if the
    /// sequence number remote sequence - n is in the received queue.
    pub fn new(
        p_type: PacketType,
        local_packet_index: u16,
        last_remote_packet_index: u16,
        bit_field: u32,
        host_tick: u16,
    ) -> StandardHeader {
        StandardHeader {
            p_type,
            local_packet_index,
            last_remote_packet_index,
            ack_field: bit_field,
            host_tick,
        }
    }

    /// Returns the number of bytes in the header
    pub const fn bytes_number() -> usize {
        return 11;
    }

    /// Returns the packet type indicated by the header
    pub fn packet_type(&self) -> PacketType {
        self.p_type
    }

    /// Returns the sequence number from this packet.
    pub fn local_packet_index(&self) -> u16 {
        self.local_packet_index
    }

    /// Returns bit field of all last 32 acknowledged packages.
    pub fn ack_field(&self) -> u32 {
        self.ack_field
    }

    /// Returns last acknowledged sequence number.
    pub fn last_remote_packet_index(&self) -> u16 {
        self.last_remote_packet_index
    }

    /// Returns the current tick of the sending Host
    pub fn host_tick(&self) -> u16 {
        self.host_tick
    }
}
