use naia_serde::{BitReader, BitWrite, Serde};

use crate::{packet_type::PacketType, standard_header::StandardHeader};

/// Strip the standard header off of a packet's payload
pub fn read_header(reader: &mut BitReader) {
    StandardHeader::de(reader).unwrap();
}
