use super::*;
use crate::webrtc::data_channel::internal::error::Error;

// The first byte in a `Message` that specifies its type:
pub(crate) const MESSAGE_TYPE_ACK: u8 = 0x02;
pub(crate) const MESSAGE_TYPE_OPEN: u8 = 0x03;
pub(crate) const MESSAGE_TYPE_LEN: usize = 1;

type Result<T> = std::result::Result<T, crate::webrtc::util::Error>;

// A parsed DataChannel message
#[derive(Eq, PartialEq, Copy, Clone, Debug)]
pub(crate) enum MessageType {
    DataChannelAck,
    DataChannelOpen,
}

impl MarshalSize for MessageType {
    fn marshal_size(&self) -> usize {
        MESSAGE_TYPE_LEN
    }
}

impl Marshal for MessageType {
    fn marshal_to(&self, mut buf: &mut [u8]) -> Result<usize> {
        let b = match self {
            MessageType::DataChannelAck => MESSAGE_TYPE_ACK,
            MessageType::DataChannelOpen => MESSAGE_TYPE_OPEN,
        };

        buf.put_u8(b);

        Ok(1)
    }
}

impl Unmarshal for MessageType {
    fn unmarshal<B>(buf: &mut B) -> Result<Self>
    where
        B: Buf,
    {
        let required_len = MESSAGE_TYPE_LEN;
        if buf.remaining() < required_len {
            return Err(Error::UnexpectedEndOfBuffer {
                expected: required_len,
                actual: buf.remaining(),
            }
            .into());
        }

        let b = buf.get_u8();

        match b {
            MESSAGE_TYPE_ACK => Ok(Self::DataChannelAck),
            MESSAGE_TYPE_OPEN => Ok(Self::DataChannelOpen),
            _ => Err(Error::InvalidMessageType(b).into()),
        }
    }
}
