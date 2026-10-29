use super::*;
use crate::webrtc::dtls::signature_hash_algorithm::*;

use byteorder::{BigEndian, ReadBytesExt, WriteBytesExt};
use std::io::{Read, Write};

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct HandshakeMessageCertificateVerify {
    pub(crate) algorithm: SignatureHashAlgorithm,
    pub(crate) signature: Vec<u8>,
}

impl HandshakeMessageCertificateVerify {
    pub(crate) fn handshake_type(&self) -> HandshakeType {
        HandshakeType::CertificateVerify
    }

    pub(crate) fn size(&self) -> usize {
        1 + 1 + 2 + self.signature.len()
    }

    pub(crate) fn marshal<W: Write>(&self, writer: &mut W) -> Result<()> {
        writer.write_u8(self.algorithm.hash as u8)?;
        writer.write_u8(self.algorithm.signature as u8)?;
        writer.write_u16::<BigEndian>(self.signature.len() as u16)?;
        writer.write_all(&self.signature)?;

        Ok(writer.flush()?)
    }

    pub(crate) fn unmarshal<R: Read>(reader: &mut R) -> Result<Self> {
        let hash_algorithm = reader.read_u8()?.into();
        let signature_algorithm = reader.read_u8()?.into();
        let signature_length = reader.read_u16::<BigEndian>()? as usize;
        let mut signature = vec![0; signature_length];
        reader.read_exact(&mut signature)?;

        Ok(HandshakeMessageCertificateVerify {
            algorithm: SignatureHashAlgorithm {
                hash: hash_algorithm,
                signature: signature_algorithm,
            },
            signature,
        })
    }
}
