use super::*;
use crate::webrtc::dtls::signature_hash_algorithm::*;

// https://tools.ietf.org/html/rfc5246#section-7.4.1.4.1
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ExtensionSupportedSignatureAlgorithms {
    pub(crate) signature_hash_algorithms: Vec<SignatureHashAlgorithm>,
}

impl ExtensionSupportedSignatureAlgorithms {
    pub(crate) fn extension_value(&self) -> ExtensionValue {
        ExtensionValue::SupportedSignatureAlgorithms
    }

    pub(crate) fn size(&self) -> usize {
        2 + 2 + self.signature_hash_algorithms.len() * 2
    }

    pub(crate) fn marshal<W: Write>(&self, writer: &mut W) -> Result<()> {
        writer.write_u16::<BigEndian>(2 + 2 * self.signature_hash_algorithms.len() as u16)?;
        writer.write_u16::<BigEndian>(2 * self.signature_hash_algorithms.len() as u16)?;
        for v in &self.signature_hash_algorithms {
            writer.write_u8(v.hash as u8)?;
            writer.write_u8(v.signature as u8)?;
        }

        Ok(writer.flush()?)
    }

    pub(crate) fn unmarshal<R: Read>(reader: &mut R) -> Result<Self> {
        let _ = reader.read_u16::<BigEndian>()?;

        let algorithm_count = reader.read_u16::<BigEndian>()? as usize / 2;
        let mut signature_hash_algorithms = vec![];
        for _ in 0..algorithm_count {
            let hash = reader.read_u8()?.into();
            let signature = reader.read_u8()?.into();
            signature_hash_algorithms.push(SignatureHashAlgorithm { hash, signature });
        }

        Ok(ExtensionSupportedSignatureAlgorithms {
            signature_hash_algorithms,
        })
    }
}
