pub(crate) mod message_channel_ack;
pub(crate) mod message_channel_open;
pub(crate) mod message_type;

use message_channel_ack::*;
use message_channel_open::*;
use message_type::*;

use crate::webrtc::data_channel::internal::error::Error;

use crate::webrtc::util::marshal::*;

use bytes::{Buf, BufMut};

/// A parsed DataChannel message
#[derive(Eq, PartialEq, Clone, Debug)]
pub(crate) enum Message {
    DataChannelAck(DataChannelAck),
    DataChannelOpen(DataChannelOpen),
}

impl MarshalSize for Message {
    fn marshal_size(&self) -> usize {
        match self {
            Message::DataChannelAck(m) => m.marshal_size() + MESSAGE_TYPE_LEN,
            Message::DataChannelOpen(m) => m.marshal_size() + MESSAGE_TYPE_LEN,
        }
    }
}

impl Marshal for Message {
    fn marshal_to(&self, mut buf: &mut [u8]) -> Result<usize, crate::webrtc::util::Error> {
        let mut bytes_written = 0;
        let n = self.message_type().marshal_to(buf)?;
        buf = &mut buf[n..];
        bytes_written += n;
        bytes_written += match self {
            Message::DataChannelAck(_) => 0,
            Message::DataChannelOpen(open) => open.marshal_to(buf)?,
        };
        Ok(bytes_written)
    }
}

impl Unmarshal for Message {
    fn unmarshal<B>(buf: &mut B) -> Result<Self, crate::webrtc::util::Error>
    where
        Self: Sized,
        B: Buf,
    {
        if buf.remaining() < MESSAGE_TYPE_LEN {
            return Err(Error::UnexpectedEndOfBuffer {
                expected: MESSAGE_TYPE_LEN,
                actual: buf.remaining(),
            }
            .into());
        }

        match MessageType::unmarshal(buf)? {
            MessageType::DataChannelAck => Ok(Self::DataChannelAck(DataChannelAck {})),
            _ => {
                panic!("should never need to unmarshal anything other than a DataChannelAck");
            }
        }
    }
}

impl Message {
    pub(crate) fn message_type(&self) -> MessageType {
        match self {
            Self::DataChannelAck(_) => MessageType::DataChannelAck,
            Self::DataChannelOpen(_) => MessageType::DataChannelOpen,
        }
    }
}
