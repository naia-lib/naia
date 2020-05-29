use crate::PacketType;
use crate::StandardHeader;

pub fn get_connectionless_payload(
    packet_type: PacketType,
    payload: &[u8],
) -> Box<[u8]> {

    // Add Ack Header onto message!
    let mut header_bytes = Vec::new();

    let header = StandardHeader::new(packet_type, 0, 0, 0);
    header.write(&mut header_bytes);

    [header_bytes.as_slice(), &payload]
        .concat()
        .into_boxed_slice()
}