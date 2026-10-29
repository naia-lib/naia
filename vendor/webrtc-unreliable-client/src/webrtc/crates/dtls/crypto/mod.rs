pub(crate) mod crypto_gcm;

use crate::webrtc::dtls::curve::named_curve::*;
use crate::webrtc::dtls::error::*;
use crate::webrtc::dtls::record_layer::record_layer_header::*;
use crate::webrtc::dtls::signature_hash_algorithm::{
    HashAlgorithm, SignatureAlgorithm, SignatureHashAlgorithm,
};

#[derive(Clone, PartialEq)]
pub(crate) struct Certificate {
    /// DER-encoded certificate chain.
    pub(crate) certificate: Vec<Vec<u8>>,
    pub(crate) private_key: CryptoPrivateKey,
}

pub(crate) fn value_key_message(
    client_random: &[u8],
    server_random: &[u8],
    public_key: &[u8],
    named_curve: NamedCurve,
) -> Vec<u8> {
    let mut server_ecdh_params = vec![0u8; 4];
    server_ecdh_params[0] = 3; // named curve
    server_ecdh_params[1..3].copy_from_slice(&(named_curve as u16).to_be_bytes());
    server_ecdh_params[3] = public_key.len() as u8;

    let mut plaintext = vec![];
    plaintext.extend_from_slice(client_random);
    plaintext.extend_from_slice(server_random);
    plaintext.extend_from_slice(&server_ecdh_params);
    plaintext.extend_from_slice(public_key);

    plaintext
}

// The client never signs anything (no CertificateVerify, no signed key
// exchange), so the private key is carried only as its PKCS#8 DER bytes plus
// a tag recording which algorithm it is.
#[derive(Clone, Copy, PartialEq)]
pub(crate) enum CryptoPrivateKeyKind {
    Ed25519,
    Ecdsa256,
    Rsa256,
}

#[derive(Clone, PartialEq)]
pub(crate) struct CryptoPrivateKey {
    pub(crate) kind: CryptoPrivateKeyKind,
    pub(crate) serialized_der: Vec<u8>,
}

fn verify_signature(
    message: &[u8],
    hash_algorithm: &SignatureHashAlgorithm,
    remote_key_signature: &[u8],
    raw_certificates: &[Vec<u8>],
) -> Result<()> {
    if raw_certificates.is_empty() {
        return Err(Error::ErrLengthMismatch);
    }

    let (_, certificate) = x509_parser::parse_x509_certificate(&raw_certificates[0])
        .map_err(|e| Error::Other(e.to_string()))?;

    let verify_alg: &dyn ring::signature::VerificationAlgorithm = match hash_algorithm.signature {
        SignatureAlgorithm::Ed25519 => &ring::signature::ED25519,
        SignatureAlgorithm::Ecdsa if hash_algorithm.hash == HashAlgorithm::Sha256 => {
            &ring::signature::ECDSA_P256_SHA256_ASN1
        }
        SignatureAlgorithm::Ecdsa if hash_algorithm.hash == HashAlgorithm::Sha384 => {
            &ring::signature::ECDSA_P384_SHA384_ASN1
        }
        SignatureAlgorithm::Rsa if hash_algorithm.hash == HashAlgorithm::Sha1 => {
            &ring::signature::RSA_PKCS1_1024_8192_SHA1_FOR_LEGACY_USE_ONLY
        }
        SignatureAlgorithm::Rsa if hash_algorithm.hash == HashAlgorithm::Sha256 => {
            &ring::signature::RSA_PKCS1_2048_8192_SHA256
        }
        SignatureAlgorithm::Rsa if hash_algorithm.hash == HashAlgorithm::Sha384 => {
            &ring::signature::RSA_PKCS1_2048_8192_SHA384
        }
        SignatureAlgorithm::Rsa if hash_algorithm.hash == HashAlgorithm::Sha512 => {
            &ring::signature::RSA_PKCS1_2048_8192_SHA512
        }
        _ => return Err(Error::ErrKeySignatureVerifyUnimplemented),
    };

    log::trace!("Picked an algorithm {:?}", verify_alg);

    let public_key = ring::signature::UnparsedPublicKey::new(
        verify_alg,
        certificate
            .tbs_certificate
            .subject_pki
            .subject_public_key
            .data,
    );

    public_key
        .verify(message, remote_key_signature)
        .map_err(|e| Error::Other(e.to_string()))?;

    Ok(())
}

pub(crate) fn verify_key_signature(
    message: &[u8],
    hash_algorithm: &SignatureHashAlgorithm,
    remote_key_signature: &[u8],
    raw_certificates: &[Vec<u8>],
) -> Result<()> {
    verify_signature(
        message,
        hash_algorithm,
        remote_key_signature,
        raw_certificates,
    )
}

pub(crate) fn generate_aead_additional_data(h: &RecordLayerHeader, payload_len: usize) -> Vec<u8> {
    let mut additional_data = vec![0u8; 13];
    // SequenceNumber MUST be set first
    // we only want uint48, clobbering an extra 2 (using uint64, rust doesn't have uint48)
    additional_data[..8].copy_from_slice(&h.sequence_number.to_be_bytes());
    additional_data[..2].copy_from_slice(&h.epoch.to_be_bytes());
    additional_data[8] = h.content_type as u8;
    additional_data[9] = h.protocol_version.major;
    additional_data[10] = h.protocol_version.minor;
    additional_data[11..].copy_from_slice(&(payload_len as u16).to_be_bytes());

    additional_data
}
