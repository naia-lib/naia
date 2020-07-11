use byteorder::{BigEndian, ReadBytesExt, WriteBytesExt};

use std::io::Read;

use crate::packet_type::PacketType;

#[derive(Copy, Clone, Debug)]
/// This header provides reliability information.
pub struct StandardHeader {
    p_type: PacketType,
    // This is the sequence number so that we can know where in the sequence of packages this
    // packet belongs.
    local_packet_index: u16,
    // This is the last acknowledged sequence number.
    last_remote_packet_index: u16,
    // This is an bitfield of all last 32 acknowledged packages
    ack_field: u32,
    // This is the current tick of the host
    current_tick: u16,
    // This is the difference between the tick of the host and the tick received from the remote
    // host
    tick_latency: u8,
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
        current_tick: u16,
        tick_latency: u8,
    ) -> StandardHeader {
        StandardHeader {
            p_type,
            local_packet_index,
            last_remote_packet_index,
            ack_field: bit_field,
            current_tick,
            tick_latency,
        }
    }

    pub const fn bytes_number() -> usize {
        return 12;
    }

    /// Returns the sequence number from this packet.
    pub fn sequence(&self) -> u16 {
        self.local_packet_index
    }

    /// Returns bit field of all last 32 acknowledged packages.
    pub fn ack_field(&self) -> u32 {
        self.ack_field
    }

    /// Returns last acknowledged sequence number.
    pub fn ack_seq(&self) -> u16 {
        self.last_remote_packet_index
    }

    /// Returns tick associated with packet
    pub fn tick(&self) -> u16 {
        self.current_tick
    }

    /// Returns tick difference between hosts, associated with packet
    pub fn tick_diff(&self) -> u8 {
        self.tick_latency
    }

    pub fn write(&self, buffer: &mut Vec<u8>) {
        buffer.write_u8(self.p_type as u8).unwrap();
        buffer
            .write_u16::<BigEndian>(self.local_packet_index)
            .unwrap();
        buffer
            .write_u16::<BigEndian>(self.last_remote_packet_index)
            .unwrap();
        buffer.write_u32::<BigEndian>(self.ack_field).unwrap();
        buffer.write_u16::<BigEndian>(self.current_tick).unwrap();
        buffer.write_u8(self.tick_latency).unwrap();
    }

    pub fn read(mut msg: &[u8]) -> (Self, Box<[u8]>) {
        let p_type: PacketType = msg.read_u8().unwrap().into();
        let seq = msg.read_u16::<BigEndian>().unwrap();
        let ack_seq = msg.read_u16::<BigEndian>().unwrap();
        let ack_field = msg.read_u32::<BigEndian>().unwrap();
        let tick = msg.read_u16::<BigEndian>().unwrap();
        let tick_diff = msg.read_u8().unwrap();

        let mut buffer = Vec::new();
        msg.read_to_end(&mut buffer).unwrap();

        (
            StandardHeader {
                p_type,
                local_packet_index: seq,
                last_remote_packet_index: ack_seq,
                ack_field,
                current_tick: tick,
                tick_latency: tick_diff,
            },
            buffer.into_boxed_slice(),
        )
    }

    pub fn get_packet_type(mut payload: &[u8]) -> PacketType {
        payload.read_u8().unwrap().into()
    }

    pub fn get_sequence(mut payload: &[u8]) -> u16 {
        let _ = payload.read_u8().unwrap();
        let seq = payload.read_u16::<BigEndian>().unwrap();
        return seq;
    }
}
