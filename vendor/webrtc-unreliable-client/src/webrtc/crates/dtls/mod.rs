pub(crate) mod alert;
pub(crate) mod application_data;
pub(crate) mod change_cipher_spec;
pub(crate) mod cipher_suite;
pub(crate) mod compression_methods;
pub(crate) mod config;
pub(crate) mod conn;
pub(crate) mod content;
pub(crate) mod crypto;
pub(crate) mod curve;
mod error;
pub(crate) mod extension;
pub(crate) mod flight;
pub(crate) mod fragment_buffer;
pub(crate) mod handshake;
pub(crate) mod handshaker;
pub(crate) mod prf;
pub(crate) mod record_layer;
pub(crate) mod signature_hash_algorithm;
pub(crate) mod state;

pub(crate) use error::Error;

use cipher_suite::*;

pub(crate) fn find_matching_cipher_suite(
    a: &[CipherSuiteId],
    b: &[CipherSuiteId],
) -> Result<CipherSuiteId, ()> {
    for a_suite in a {
        for b_suite in b {
            if a_suite == b_suite {
                return Ok(*a_suite);
            }
        }
    }
    Err(())
}
