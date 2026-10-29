use super::*;

use std::io::{Read, Write};

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct HandshakeMessageServerHelloDone;

impl HandshakeMessageServerHelloDone {
    pub(crate) fn handshake_type(&self) -> HandshakeType {
        HandshakeType::ServerHelloDone
    }

    pub(crate) fn size(&self) -> usize {
        0
    }

    pub(crate) fn marshal<W: Write>(&self, _writer: &mut W) -> Result<()> {
        Ok(())
    }

    pub(crate) fn unmarshal<R: Read>(_reader: &mut R) -> Result<Self> {
        Ok(HandshakeMessageServerHelloDone {})
    }
}
