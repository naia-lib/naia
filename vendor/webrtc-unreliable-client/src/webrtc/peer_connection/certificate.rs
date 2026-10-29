use crate::webrtc::dtls_transport::dtls_fingerprint::RTCDtlsFingerprint;
use crate::webrtc::error::{Error, Result};
use crate::webrtc::peer_connection::math_rand_alpha;

use crate::webrtc::dtls::crypto::{CryptoPrivateKey, CryptoPrivateKeyKind};
use rcgen::{CertificateParams, Error as RcgenError, KeyPair};
use sha2::{Digest, Sha256};

/// Certificate represents a x509Cert used to authenticate WebRTC communications.
pub(crate) struct RTCCertificate {
    pub(crate) certificate: crate::webrtc::dtls::crypto::Certificate,
}

/// Equals determines if two certificates are identical by comparing only certificate
impl PartialEq for RTCCertificate {
    fn eq(&self, other: &Self) -> bool {
        self.certificate == other.certificate
    }
}

impl RTCCertificate {
    /// from_params generates a new x509 compliant Certificate to be used
    /// by DTLS for encrypting data sent over the wire. This method differs from
    /// generate_certificate by allowing to specify a template x509.Certificate to
    /// be used in order to define certificate parameters.
    pub(crate) fn from_params(mut params: CertificateParams) -> Result<Self> {
        let key_pair = if let Some(key_pair) = params.key_pair.take() {
            if !key_pair.is_compatible(params.alg) {
                return Err(RcgenError::CertificateKeyPairMismatch.into());
            }
            key_pair
        } else {
            KeyPair::generate(params.alg)?
        };

        let serialized_der = key_pair.serialize_der();
        let kind = if key_pair.is_compatible(&rcgen::PKCS_ED25519) {
            CryptoPrivateKeyKind::Ed25519
        } else if key_pair.is_compatible(&rcgen::PKCS_ECDSA_P256_SHA256) {
            CryptoPrivateKeyKind::Ecdsa256
        } else if key_pair.is_compatible(&rcgen::PKCS_RSA_SHA256) {
            CryptoPrivateKeyKind::Rsa256
        } else {
            return Err(Error::new("Unsupported key_pair".to_owned()));
        };
        let private_key = CryptoPrivateKey {
            kind,
            serialized_der,
        };
        params.key_pair = Some(key_pair);

        let x509_cert = rcgen::Certificate::from_params(params)?;
        let certificate = x509_cert.serialize_der()?;

        Ok(RTCCertificate {
            certificate: crate::webrtc::dtls::crypto::Certificate {
                certificate: vec![certificate],
                private_key,
            },
        })
    }

    /// get_fingerprints returns certificate fingerprints, one of which
    /// is computed with the digest algorithm used in the certificate signature.
    pub(crate) fn get_fingerprints(&self) -> Result<Vec<RTCDtlsFingerprint>> {
        let mut fingerpints = vec![];

        for certificate in &self.certificate.certificate {
            let mut h = Sha256::new();
            h.update(certificate);
            let hashed = h.finalize();
            let values: Vec<String> = hashed.iter().map(|x| format!("{:02x}", x)).collect();

            fingerpints.push(RTCDtlsFingerprint {
                algorithm: "sha-256".to_owned(),
                value: values.join(":"),
            });
        }

        Ok(fingerpints)
    }

    /// from_key_pair causes the creation of an X.509 certificate and
    /// corresponding private key.
    pub(crate) fn from_key_pair(key_pair: KeyPair) -> Result<Self> {
        let mut params = CertificateParams::new(vec![math_rand_alpha(16)]);

        if key_pair.is_compatible(&rcgen::PKCS_ED25519) {
            params.alg = &rcgen::PKCS_ED25519;
        } else if key_pair.is_compatible(&rcgen::PKCS_ECDSA_P256_SHA256) {
            params.alg = &rcgen::PKCS_ECDSA_P256_SHA256;
        } else if key_pair.is_compatible(&rcgen::PKCS_RSA_SHA256) {
            params.alg = &rcgen::PKCS_RSA_SHA256;
        } else {
            return Err(Error::new("Unsupported key_pair".to_owned()));
        };
        params.key_pair = Some(key_pair);

        RTCCertificate::from_params(params)
    }
}
