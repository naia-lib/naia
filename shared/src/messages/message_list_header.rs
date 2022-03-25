use naia_serde::{BitReader, BitWrite, Serde, UnsignedVariableInteger};

pub fn write_list_header<S: BitWrite>(writer: &mut S, mut message_count: u16) {
    let has_messages: bool = message_count > 0;
    has_messages.ser(writer);

    // write number of messages
    if has_messages {
        // we already know messages isn't 0, so you can send the count as a value >= 1
        message_count -= 1;

        let serde_count = UnsignedVariableInteger::<3>::new(message_count);

        serde_count.ser(writer);
    }
}

pub fn read_list_header(reader: &mut BitReader) -> u16 {
    let has_messages = bool::de(reader).unwrap();

    if has_messages {
        let serde_count = UnsignedVariableInteger::<3>::de(reader).unwrap();

        let mut message_count = serde_count.get() as u16;

        // we already know messages isn't 0, so you can send the count as a value >= 1
        message_count += 1;

        return message_count;
    } else {
        return 0;
    }
}
