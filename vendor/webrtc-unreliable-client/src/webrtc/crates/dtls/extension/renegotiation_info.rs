use super::*;
use crate::webrtc::dtls::error::Error::ErrInvalidPacketLength;

/// RenegotiationInfo allows a Client/Server to
/// communicate their renegotation support
/// https://tools.ietf.org/html/rfc5746
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ExtensionRenegotiationInfo {
    pub(crate) renegotiated_connection: u8,
}

impl ExtensionRenegotiationInfo {
    // TypeValue returns the extension TypeValue
    pub(crate) fn extension_value(&self) -> ExtensionValue {
        ExtensionValue::RenegotiationInfo
    }

    pub(crate) fn size(&self) -> usize {
        3
    }

    /// marshal encodes the extension
    pub(crate) fn marshal<W: Write>(&self, writer: &mut W) -> Result<()> {
        writer.write_u16::<BigEndian>(1)?; //length
        writer.write_u8(self.renegotiated_connection)?;

        Ok(writer.flush()?)
    }

    /// Unmarshal populates the extension from encoded data
    pub(crate) fn unmarshal<R: Read>(reader: &mut R) -> Result<Self> {
        let l = reader.read_u16::<BigEndian>()?; //length
        if l != 1 {
            return Err(ErrInvalidPacketLength);
        }

        let renegotiated_connection = reader.read_u8()?;

        Ok(ExtensionRenegotiationInfo {
            renegotiated_connection,
        })
    }
}
