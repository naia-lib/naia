use crate::webrtc::dtls::cipher_suite::*;
use crate::webrtc::dtls::crypto::*;
use crate::webrtc::dtls::error::*;

use std::time::Duration;

/// Config is used to configure a DTLS client.
/// After a Config is passed to a DTLS function it must not be modified.
///
/// This port only runs the WebRTC client role against a fingerprint-
/// authenticated server, so the upstream PSK, client-auth policy, SRTP
/// negotiation, and rustls/webpki chain-verification knobs have been removed.
#[derive(Clone, Default)]
pub(crate) struct Config {
    /// certificates contains certificate chain to present to the other side of the connection.
    pub(crate) certificates: Vec<Certificate>,

    /// cipher_suites is a list of supported cipher suites.
    /// If cipher_suites is nil, a default list is used
    pub(crate) cipher_suites: Vec<CipherSuiteId>,

    /// extended_master_secret determines if the "Extended Master Secret" extension
    /// should be disabled, requested, or required (default requested).
    pub(crate) extended_master_secret: ExtendedMasterSecretType,

    /// flight_interval controls how often we send outbound handshake messages
    /// defaults to time.Second
    pub(crate) flight_interval: Duration,

    /// server_name is carried in the SNI extension.
    pub(crate) server_name: String,

    /// mtu is the length at which handshake messages will be fragmented to
    /// fit within the maximum transmission unit (default is 1200 bytes)
    pub(crate) mtu: usize,

    /// replay_protection_window is the size of the replay attack protection window.
    /// Duplication of the sequence number is checked in this window size.
    /// Packet with sequence number older than this value compared to the latest
    /// accepted packet will be discarded. (default is 64)
    pub(crate) replay_protection_window: usize,
}

pub(crate) const DEFAULT_MTU: usize = 1200; // bytes

// ExtendedMasterSecretType declares the policy the client and server
// will follow for the Extended Master Secret extension
#[derive(PartialEq, Copy, Clone, Default)]
pub(crate) enum ExtendedMasterSecretType {
    #[default]
    Request = 0,
    Require = 1,
    Disable = 2,
}

pub(crate) fn validate_config(_is_client: bool, config: &Config) -> Result<()> {
    for cert in &config.certificates {
        match cert.private_key.kind {
            CryptoPrivateKeyKind::Ed25519 => {}
            CryptoPrivateKeyKind::Ecdsa256 => {}
            _ => return Err(Error::ErrInvalidPrivateKey),
        }
    }

    parse_cipher_suites(&config.cipher_suites)?;

    Ok(())
}
