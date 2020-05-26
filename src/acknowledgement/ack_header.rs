
use byteorder::{BigEndian, ReadBytesExt, WriteBytesExt};

use log::{info};

pub const ACKED_PACKET_HEADER_SIZE: u8 = 8;

#[derive(Copy, Clone, Debug)]
/// This header provides reliability information.
pub struct AckHeader {
    /// This is the sequence number so that we can know where in the sequence of packages this packet belongs.
    pub seq: u16,
    // This is the last acknowledged sequence number.
    ack_seq: u16,
    // This is an bitfield of all last 32 acknowledged packages
    ack_field: u32,
}

impl AckHeader {
    /// When we compose packet headers, the local sequence becomes the sequence number of the packet, and the remote sequence becomes the ack.
    /// The ack bitfield is calculated by looking into a queue of up to 33 packets, containing sequence numbers in the range [remote sequence - 32, remote sequence].
    /// We set bit n (in [1,32]) in ack bits to 1 if the sequence number remote sequence - n is in the received queue.
    pub fn new(seq_num: u16, last_seq: u16, bit_field: u32) -> AckHeader {
        AckHeader {
            seq: seq_num,
            ack_seq: last_seq,
            ack_field: bit_field,
        }
    }

    /// Returns the sequence number from this packet.
    #[allow(dead_code)]
    pub fn sequence(&self) -> u16 {
        self.seq
    }

    /// Returns bit field of all last 32 acknowledged packages.
    pub fn ack_field(&self) -> u32 {
        self.ack_field
    }

    /// Returns last acknowledged sequence number.
    pub fn ack_seq(&self) -> u16 {
        self.ack_seq
    }

    pub fn write(&self, buffer: &mut Vec<u8>) {
        buffer.write_u16::<BigEndian>(self.seq).unwrap();
        buffer.write_u16::<BigEndian>(self.ack_seq).unwrap();
        buffer.write_u32::<BigEndian>(self.ack_field).unwrap();
    }

    pub fn read(mut msg: &[u8]) -> (Self, Box<[u8]>) {

        let seq = msg.read_u16::<BigEndian>().unwrap();
        let ack_seq = msg.read_u16::<BigEndian>().unwrap();
        let ack_field = msg.read_u32::<BigEndian>().unwrap();

        info!("READING HEADER {}, {}, {}", seq, ack_seq, ack_field);

        let boxed = msg[8..].to_vec().into_boxed_slice();

        (AckHeader {
            seq,
            ack_seq,
            ack_field,
        }, boxed)
    }

    fn size() -> u8 {
        ACKED_PACKET_HEADER_SIZE
    }
}