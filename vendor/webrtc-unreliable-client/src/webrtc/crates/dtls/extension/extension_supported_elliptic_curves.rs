use super::*;
use crate::webrtc::dtls::curve::named_curve::*;

// https://tools.ietf.org/html/rfc8422#section-5.1.1
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ExtensionSupportedEllipticCurves {
    pub(crate) elliptic_curves: Vec<NamedCurve>,
}

impl ExtensionSupportedEllipticCurves {
    pub(crate) fn extension_value(&self) -> ExtensionValue {
        ExtensionValue::SupportedEllipticCurves
    }

    pub(crate) fn size(&self) -> usize {
        2 + 2 + self.elliptic_curves.len() * 2
    }

    pub(crate) fn marshal<W: Write>(&self, writer: &mut W) -> Result<()> {
        writer.write_u16::<BigEndian>(2 + 2 * self.elliptic_curves.len() as u16)?;
        writer.write_u16::<BigEndian>(2 * self.elliptic_curves.len() as u16)?;
        for v in &self.elliptic_curves {
            writer.write_u16::<BigEndian>(*v as u16)?;
        }

        Ok(writer.flush()?)
    }

    pub(crate) fn unmarshal<R: Read>(reader: &mut R) -> Result<Self> {
        let _ = reader.read_u16::<BigEndian>()?;

        let group_count = reader.read_u16::<BigEndian>()? as usize / 2;
        let mut elliptic_curves = vec![];
        for _ in 0..group_count {
            let elliptic_curve = reader.read_u16::<BigEndian>()?.into();
            elliptic_curves.push(elliptic_curve);
        }

        Ok(ExtensionSupportedEllipticCurves { elliptic_curves })
    }
}
