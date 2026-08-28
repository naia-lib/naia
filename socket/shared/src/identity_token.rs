use naia_serde::{BitReader, BitWrite, Serde, SerdeErr};

use crate::Random;

/// Number of random bytes in a freshly generated token.
const TOKEN_LEN: usize = 32;

/// An opaque identity token.
///
/// The server mints one of these per accepted authentication and hands it to
/// the client over the signaling channel; the client then presents it in-band
/// during the handshake so the server can bind the incoming socket address to
/// an already-authenticated user.
///
/// The bytes are opaque: nothing outside the crate that minted a token should
/// interpret, parse, or construct token contents. Because signaling travels
/// over HTTP as text, [`IdentityToken::to_signaling_string`] and
/// [`IdentityToken::from_signaling_string`] provide a base64 (URL-safe, no
/// padding) text encoding for that hop only.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct IdentityToken(Box<[u8]>);

impl IdentityToken {
    /// Mints a new random token.
    pub fn generate() -> Self {
        let mut bytes = Vec::with_capacity(TOKEN_LEN);
        for _ in 0..TOKEN_LEN {
            bytes.push(Random::gen_range_u32(0, 256) as u8);
        }
        Self(bytes.into_boxed_slice())
    }

    /// Wraps raw bytes as a token. No validation is performed: any byte
    /// sequence is a syntactically valid token, and a token is only ever
    /// meaningful by comparison against one the server minted.
    pub fn from_bytes(bytes: Vec<u8>) -> Self {
        Self(bytes.into_boxed_slice())
    }

    /// The token's raw bytes.
    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }

    /// Number of bytes in the token.
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Whether the token carries no bytes at all.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Encodes the token for the text-based signaling hop.
    pub fn to_signaling_string(&self) -> String {
        base64::encode_config(&self.0, base64::URL_SAFE_NO_PAD)
    }

    /// Decodes a token received over the text-based signaling hop.
    pub fn from_signaling_string(string: &str) -> Option<Self> {
        base64::decode_config(string, base64::URL_SAFE_NO_PAD)
            .ok()
            .map(|bytes| Self(bytes.into_boxed_slice()))
    }
}

impl Serde for IdentityToken {
    fn ser(&self, writer: &mut dyn BitWrite) {
        self.0.ser(writer);
    }

    fn de(reader: &mut BitReader) -> Result<Self, SerdeErr> {
        Ok(Self(Box::<[u8]>::de(reader)?))
    }

    fn bit_length(&self) -> u32 {
        self.0.bit_length()
    }
}

#[cfg(test)]
mod tests {
    use naia_serde::{BitReader, BitWriter, Serde};

    use super::IdentityToken;

    #[test]
    fn signaling_string_round_trip() {
        let token = IdentityToken::generate();
        let encoded = token.to_signaling_string();
        assert_eq!(Some(token), IdentityToken::from_signaling_string(&encoded));
    }

    #[test]
    fn serde_round_trip() {
        let token = IdentityToken::generate();

        let mut writer = BitWriter::new();
        token.ser(&mut writer);
        let bytes = writer.to_bytes();

        let mut reader = BitReader::new(&bytes);
        assert_eq!(token, IdentityToken::de(&mut reader).unwrap());
    }
}
