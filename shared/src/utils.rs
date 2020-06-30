use crate::{packet_type::PacketType, standard_header::StandardHeader};

/// Write a connectionless packet, that is, one that does not rely on information normally retrieved from the connection
pub fn write_connectionless_payload(packet_type: PacketType, payload: &[u8]) -> Box<[u8]> {
    // Add Ack Header onto message!
    let mut header_bytes = Vec::new();

    let header = StandardHeader::new(packet_type, 0, 0, 0);
    header.write(&mut header_bytes);

    [header_bytes.as_slice(), &payload]
        .concat()
        .into_boxed_slice()
}

/// Strip the standard header off of a packet's payload and retrieve the payload bytes
pub fn read_headerless_payload(payload: &[u8]) -> Box<[u8]> {
    let (_, stripped_message) = StandardHeader::read(payload);
    stripped_message
}
