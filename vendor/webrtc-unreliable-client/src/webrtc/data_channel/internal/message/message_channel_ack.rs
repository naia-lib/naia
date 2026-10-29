use super::*;

type Result<T> = std::result::Result<T, crate::webrtc::util::Error>;

/// The data-part of an data-channel ACK message without the message type.
///
/// # Memory layout
///
/// ```plain
/// 0                   1                   2                   3
/// 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1
///+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
///|  Message Type |
///+-+-+-+-+-+-+-+-+
/// ```
#[derive(Eq, PartialEq, Clone, Debug)]
pub(crate) struct DataChannelAck;

impl MarshalSize for DataChannelAck {
    fn marshal_size(&self) -> usize {
        0
    }
}

impl Marshal for DataChannelAck {
    fn marshal_to(&self, _buf: &mut [u8]) -> Result<usize> {
        Ok(0)
    }
}

impl Unmarshal for DataChannelAck {
    fn unmarshal<B>(_buf: &mut B) -> Result<Self>
    where
        Self: Sized,
        B: Buf,
    {
        Ok(Self)
    }
}
