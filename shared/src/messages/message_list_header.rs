use naia_serde::{BitReader, BitWrite, Serde, SerdeErr, UnsignedVariableInteger};

pub fn write<S: BitWrite, T: Into<i128>>(writer: &mut S, message_count: T) {
    let mut message_count_i128: i128 = message_count.into();
    let has_messages: bool = message_count_i128 > 0;
    has_messages.ser(writer);

    // write number of messages
    if has_messages {
        // we already know messages isn't 0, so you can send the count as a value >= 1
        message_count_i128 -= 1;

        let serde_count = UnsignedVariableInteger::<3>::new(message_count_i128);

        serde_count.ser(writer);
    }
}

pub fn read(reader: &mut BitReader) -> Result<u16, SerdeErr> {
    let has_messages = bool::de(reader)?;

    if has_messages {
        let serde_count = UnsignedVariableInteger::<3>::de(reader)?;

        let mut message_count = serde_count.get() as u16;

        // we already know messages isn't 0, so you can send the count as a value >= 1
        message_count += 1;

        Ok(message_count)
    } else {
        Ok(0)
    }
}
