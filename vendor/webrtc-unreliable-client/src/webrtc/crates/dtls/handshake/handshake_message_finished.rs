use super::*;

use std::io::{Read, Write};

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct HandshakeMessageFinished {
    pub(crate) verify_data: Vec<u8>,
}

impl HandshakeMessageFinished {
    pub(crate) fn handshake_type(&self) -> HandshakeType {
        HandshakeType::Finished
    }

    pub(crate) fn size(&self) -> usize {
        self.verify_data.len()
    }

    pub(crate) fn marshal<W: Write>(&self, writer: &mut W) -> Result<()> {
        writer.write_all(&self.verify_data)?;

        Ok(writer.flush()?)
    }

    pub(crate) fn unmarshal<R: Read>(reader: &mut R) -> Result<Self> {
        let mut verify_data: Vec<u8> = vec![];
        reader.read_to_end(&mut verify_data)?;

        Ok(HandshakeMessageFinished { verify_data })
    }
}
