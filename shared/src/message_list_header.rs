use naia_serde::{BitReader, BitWrite, Serde};

pub fn write_list_header<S: BitWrite>(writer: &mut S, message_count: &u16) {
    let has_messages: bool = *message_count > 0;
    has_messages.ser(writer);

    // write number of messages
    if has_messages {
        message_count.ser(writer);
    }
}

pub fn read_list_header(reader: &mut BitReader) -> u16 {
    let has_messages = bool::de(reader).unwrap();

    if has_messages {
        return u16::de(reader).unwrap();
    } else {
        return 0;
    }
}