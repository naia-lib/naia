use super::*;

// https://tools.ietf.org/html/rfc8422
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ExtensionUseExtendedMasterSecret {
    pub(crate) supported: bool,
}

impl ExtensionUseExtendedMasterSecret {
    pub(crate) fn extension_value(&self) -> ExtensionValue {
        ExtensionValue::UseExtendedMasterSecret
    }

    pub(crate) fn size(&self) -> usize {
        2
    }

    pub(crate) fn marshal<W: Write>(&self, writer: &mut W) -> Result<()> {
        // length
        writer.write_u16::<BigEndian>(0)?;

        Ok(writer.flush()?)
    }

    pub(crate) fn unmarshal<R: Read>(reader: &mut R) -> Result<Self> {
        let _ = reader.read_u16::<BigEndian>()?;

        Ok(ExtensionUseExtendedMasterSecret { supported: true })
    }
}
