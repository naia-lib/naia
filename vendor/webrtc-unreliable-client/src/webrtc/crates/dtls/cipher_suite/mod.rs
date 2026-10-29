pub(crate) mod cipher_suite_aes_128_gcm_sha256;

use std::fmt;
use std::marker::{Send, Sync};

use super::error::*;
use super::record_layer::record_layer_header::*;

use cipher_suite_aes_128_gcm_sha256::*;

// CipherSuiteID is an ID for our supported CipherSuites
// Supported Cipher Suites
#[allow(non_camel_case_types)]
#[derive(Copy, Clone, Debug, PartialEq)]
pub(crate) enum CipherSuiteId {
    // AES-128-GCM-SHA256
    Tls_Ecdhe_Ecdsa_With_Aes_128_Gcm_Sha256 = 0xc02b,
    Tls_Ecdhe_Rsa_With_Aes_128_Gcm_Sha256 = 0xc02f,

    Unsupported,
}

impl fmt::Display for CipherSuiteId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            CipherSuiteId::Tls_Ecdhe_Ecdsa_With_Aes_128_Gcm_Sha256 => {
                write!(f, "TLS_ECDHE_ECDSA_WITH_AES_128_GCM_SHA256")
            }
            CipherSuiteId::Tls_Ecdhe_Rsa_With_Aes_128_Gcm_Sha256 => {
                write!(f, "TLS_ECDHE_RSA_WITH_AES_128_GCM_SHA256")
            }
            _ => write!(f, "Unsupported CipherSuiteID"),
        }
    }
}

impl From<u16> for CipherSuiteId {
    fn from(val: u16) -> Self {
        match val {
            // AES-128-GCM-SHA256
            0xc02b => CipherSuiteId::Tls_Ecdhe_Ecdsa_With_Aes_128_Gcm_Sha256,
            0xc02f => CipherSuiteId::Tls_Ecdhe_Rsa_With_Aes_128_Gcm_Sha256,

            _ => CipherSuiteId::Unsupported,
        }
    }
}

#[derive(Copy, Clone, Debug)]
pub(crate) enum CipherSuiteHash {
    Sha256,
}

impl CipherSuiteHash {
    pub(crate) fn size(&self) -> usize {
        match *self {
            CipherSuiteHash::Sha256 => 32,
        }
    }
}

pub(crate) trait CipherSuite {
    fn to_string(&self) -> String;
    fn id(&self) -> CipherSuiteId;
    fn hash_func(&self) -> CipherSuiteHash;
    fn is_initialized(&self) -> bool;

    // Generate the internal encryption state
    fn init(
        &mut self,
        master_secret: &[u8],
        client_random: &[u8],
        server_random: &[u8],
        is_client: bool,
    ) -> Result<()>;

    fn encrypt(&self, pkt_rlh: &RecordLayerHeader, raw: &[u8]) -> Result<Vec<u8>>;
    fn decrypt(&self, input: &[u8]) -> Result<Vec<u8>>;
}

// Taken from https://www.iana.org/assignments/tls-parameters/tls-parameters.xml
// A cipher_suite is a specific combination of key agreement, cipher and MAC
// function.
pub(crate) fn cipher_suite_for_id(id: CipherSuiteId) -> Result<Box<dyn CipherSuite + Send + Sync>> {
    match id {
        CipherSuiteId::Tls_Ecdhe_Ecdsa_With_Aes_128_Gcm_Sha256 => {
            Ok(Box::new(CipherSuiteAes128GcmSha256::new(false)))
        }
        CipherSuiteId::Tls_Ecdhe_Rsa_With_Aes_128_Gcm_Sha256 => {
            Ok(Box::new(CipherSuiteAes128GcmSha256::new(true)))
        }
        _ => Err(Error::ErrInvalidCipherSuite),
    }
}

// CipherSuites we support in order of preference: AES-128-GCM-SHA256 only
// (ECDSA- and RSA-keyed). This is what webrtc-unreliable (openssl) negotiates;
// the CCM/CBC/PSK suites the upstream pion port carried were never selected
// and have been removed.
pub(crate) fn default_cipher_suites() -> Vec<Box<dyn CipherSuite + Send + Sync>> {
    vec![
        Box::new(CipherSuiteAes128GcmSha256::new(false)),
        Box::new(CipherSuiteAes128GcmSha256::new(true)),
    ]
}

fn cipher_suites_for_ids(ids: &[CipherSuiteId]) -> Result<Vec<Box<dyn CipherSuite + Send + Sync>>> {
    let mut cipher_suites = vec![];
    for id in ids {
        cipher_suites.push(cipher_suite_for_id(*id)?);
    }
    Ok(cipher_suites)
}

pub(crate) fn parse_cipher_suites(
    user_selected_suites: &[CipherSuiteId],
) -> Result<Vec<Box<dyn CipherSuite + Send + Sync>>> {
    let cipher_suites = if !user_selected_suites.is_empty() {
        cipher_suites_for_ids(user_selected_suites)?
    } else {
        default_cipher_suites()
    };

    if cipher_suites.is_empty() {
        Err(Error::ErrNoAvailableCipherSuites)
    } else {
        Ok(cipher_suites)
    }
}
