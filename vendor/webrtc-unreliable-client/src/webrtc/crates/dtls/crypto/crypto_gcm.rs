// AES-GCM (Galois Counter Mode)
// The most widely used block cipher worldwide.
// Mandatory as of TLS 1.2 (2008) and used by default by most clients.
// RFC 5288 year 2008 https://tools.ietf.org/html/rfc5288

// https://github.com/RustCrypto/AEADs
// https://docs.rs/aes-gcm/0.8.0/aes_gcm/

use rand::Rng;

use std::io::Cursor;

use super::*;
use crate::webrtc::dtls::content::*;
use crate::webrtc::dtls::error::*;
use crate::webrtc::dtls::record_layer::record_layer_header::*;

use aes_gcm::aead::AeadInPlace;
use aes_gcm::{Aes128Gcm, KeyInit, Nonce};

const CRYPTO_GCM_TAG_LENGTH: usize = 16;
const CRYPTO_GCM_NONCE_LENGTH: usize = 12;

// State needed to handle encrypted input/output
#[derive(Clone)]
pub(crate) struct CryptoGcm {
    local_gcm: Aes128Gcm,
    remote_gcm: Aes128Gcm,
    local_write_iv: Vec<u8>,
    remote_write_iv: Vec<u8>,
}

impl CryptoGcm {
    pub(crate) fn new(
        local_key: &[u8],
        local_write_iv: &[u8],
        remote_key: &[u8],
        remote_write_iv: &[u8],
    ) -> Self {
        let local_gcm = Aes128Gcm::new_from_slice(local_key).unwrap();
        let remote_gcm = Aes128Gcm::new_from_slice(remote_key).unwrap();

        CryptoGcm {
            local_gcm,
            local_write_iv: local_write_iv.to_vec(),
            remote_gcm,
            remote_write_iv: remote_write_iv.to_vec(),
        }
    }

    pub(crate) fn encrypt(&self, pkt_rlh: &RecordLayerHeader, raw: &[u8]) -> Result<Vec<u8>> {
        let payload = &raw[RECORD_LAYER_HEADER_SIZE..];
        let raw = &raw[..RECORD_LAYER_HEADER_SIZE];

        let mut nonce = [0u8; CRYPTO_GCM_NONCE_LENGTH];
        nonce[..4].copy_from_slice(&self.local_write_iv[..4]);
        rand::thread_rng().fill(&mut nonce[4..]);
        let nonce = Nonce::from(nonce);

        let additional_data = generate_aead_additional_data(pkt_rlh, payload.len());

        let mut buffer: Vec<u8> = Vec::new();
        buffer.extend_from_slice(payload);

        self.local_gcm
            .encrypt_in_place(&nonce, &additional_data, &mut buffer)
            .map_err(|e| Error::Other(e.to_string()))?;

        let mut r = Vec::with_capacity(raw.len() + nonce.len() + buffer.len());
        r.extend_from_slice(raw);
        r.extend_from_slice(&nonce[4..]);
        r.extend_from_slice(&buffer);

        // Update recordLayer size to include explicit nonce
        let r_len = (r.len() - RECORD_LAYER_HEADER_SIZE) as u16;
        r[RECORD_LAYER_HEADER_SIZE - 2..RECORD_LAYER_HEADER_SIZE]
            .copy_from_slice(&r_len.to_be_bytes());

        Ok(r)
    }

    pub(crate) fn decrypt(&self, r: &[u8]) -> Result<Vec<u8>> {
        let mut reader = Cursor::new(r);
        let h = RecordLayerHeader::unmarshal(&mut reader)?;
        if h.content_type == ContentType::ChangeCipherSpec {
            // Nothing to encrypt with ChangeCipherSpec
            return Ok(r.to_vec());
        }

        if r.len() <= (RECORD_LAYER_HEADER_SIZE + 8) {
            return Err(Error::ErrNotEnoughRoomForNonce);
        }

        let mut nonce = [0u8; CRYPTO_GCM_NONCE_LENGTH];
        nonce[..4].copy_from_slice(&self.remote_write_iv[..4]);
        nonce[4..].copy_from_slice(&r[RECORD_LAYER_HEADER_SIZE..RECORD_LAYER_HEADER_SIZE + 8]);
        let nonce = Nonce::from(nonce);

        let out = &r[RECORD_LAYER_HEADER_SIZE + 8..];

        let additional_data = generate_aead_additional_data(&h, out.len() - CRYPTO_GCM_TAG_LENGTH);

        let mut buffer: Vec<u8> = Vec::new();
        buffer.extend_from_slice(out);

        self.remote_gcm
            .decrypt_in_place(&nonce, &additional_data, &mut buffer)
            .map_err(|e| Error::Other(e.to_string()))?;

        let mut d = Vec::with_capacity(RECORD_LAYER_HEADER_SIZE + buffer.len());
        d.extend_from_slice(&r[..RECORD_LAYER_HEADER_SIZE]);
        d.extend_from_slice(&buffer);

        Ok(d)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_record(payload: &[u8]) -> (RecordLayerHeader, Vec<u8>) {
        let h = RecordLayerHeader {
            content_type: ContentType::ApplicationData,
            protocol_version: PROTOCOL_VERSION1_2,
            epoch: 1,
            sequence_number: 7,
            content_len: payload.len() as u16,
        };
        let mut raw = Vec::new();
        h.marshal(&mut raw).unwrap();
        raw.extend_from_slice(payload);
        (h, raw)
    }

    /// Same key material on both directions, so what the "local" side seals is
    /// exactly what the "remote" side is set up to open.
    fn loopback_gcm() -> CryptoGcm {
        let key = [0x11u8; 16];
        let iv = [0x22u8; 4];
        CryptoGcm::new(&key, &iv, &key, &iv)
    }

    #[test]
    fn gcm_round_trip() {
        let gcm = loopback_gcm();
        let payload = b"the quick brown fox";
        let (h, raw) = test_record(payload);

        let sealed = gcm.encrypt(&h, &raw).unwrap();
        assert_ne!(&sealed[RECORD_LAYER_HEADER_SIZE..], &payload[..]);
        // explicit nonce (8) + ciphertext + tag (16)
        assert_eq!(
            sealed.len(),
            RECORD_LAYER_HEADER_SIZE + 8 + payload.len() + CRYPTO_GCM_TAG_LENGTH
        );

        let opened = gcm.decrypt(&sealed).unwrap();
        assert_eq!(&opened[RECORD_LAYER_HEADER_SIZE..], &payload[..]);
    }

    #[test]
    fn gcm_rejects_tampered_ciphertext() {
        let gcm = loopback_gcm();
        let (h, raw) = test_record(b"authenticate me");

        let mut sealed = gcm.encrypt(&h, &raw).unwrap();
        let last = sealed.len() - 1;
        sealed[last] ^= 0xff;

        assert!(gcm.decrypt(&sealed).is_err());
    }

    /// The record header is authenticated as additional data, not encrypted.
    /// If AAD generation is ever decoupled from the header actually on the
    /// wire, this is the test that catches it -- the payload would still
    /// decrypt cleanly under a forged epoch/sequence number.
    #[test]
    fn gcm_binds_the_record_header_as_aad() {
        let gcm = loopback_gcm();
        let (h, raw) = test_record(b"bound to its header");

        let mut sealed = gcm.encrypt(&h, &raw).unwrap();
        // Flip a bit in the epoch, which lives in the header and is covered by
        // the AAD but not by the ciphertext.
        sealed[3] ^= 0x01;

        assert!(gcm.decrypt(&sealed).is_err());
    }

    #[test]
    fn gcm_rejects_record_too_short_for_the_explicit_nonce() {
        let gcm = loopback_gcm();
        let (h, raw) = test_record(b"x");

        let sealed = gcm.encrypt(&h, &raw).unwrap();
        // Truncate into the 8-byte explicit nonce that follows the header.
        let truncated = &sealed[..RECORD_LAYER_HEADER_SIZE + 4];

        assert!(matches!(
            gcm.decrypt(truncated),
            Err(Error::ErrNotEnoughRoomForNonce)
        ));
    }
}
