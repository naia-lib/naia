use super::cipher_suite::*;
use super::curve::named_curve::*;
use super::handshake::handshake_random::*;

use async_lock::Mutex;
use std::marker::{Send, Sync};
use std::sync::atomic::AtomicU16;
use std::sync::Arc;

// State holds the dtls connection state and implements both encoding.BinaryMarshaler and encoding.BinaryUnmarshaler
pub(crate) struct State {
    pub(crate) local_epoch: Arc<AtomicU16>,
    pub(crate) remote_epoch: Arc<AtomicU16>,
    pub(crate) local_sequence_number: Arc<Mutex<Vec<u64>>>, // uint48
    pub(crate) local_random: HandshakeRandom,
    pub(crate) remote_random: HandshakeRandom,
    pub(crate) master_secret: Vec<u8>,
    pub(crate) cipher_suite: Arc<Mutex<Option<Box<dyn CipherSuite + Send + Sync>>>>, // nil if a cipher_suite hasn't been chosen

    pub(crate) peer_certificates: Vec<Vec<u8>>,

    pub(crate) is_client: bool,

    pub(crate) pre_master_secret: Vec<u8>,
    pub(crate) extended_master_secret: bool,

    pub(crate) named_curve: NamedCurve,
    pub(crate) local_keypair: Option<NamedCurveKeypair>,
    pub(crate) cookie: Vec<u8>,
    pub(crate) handshake_send_sequence: isize,
    pub(crate) handshake_recv_sequence: isize,
    pub(crate) local_verify_data: Vec<u8>, // cached VerifyData
}

impl Default for State {
    fn default() -> Self {
        State {
            local_epoch: Arc::new(AtomicU16::new(0)),
            remote_epoch: Arc::new(AtomicU16::new(0)),
            local_sequence_number: Arc::new(Mutex::new(vec![])),
            local_random: HandshakeRandom::default(),
            remote_random: HandshakeRandom::default(),
            master_secret: vec![],
            cipher_suite: Arc::new(Mutex::new(None)), // nil if a cipher_suite hasn't been chosen

            peer_certificates: vec![],

            is_client: false,

            pre_master_secret: vec![],
            extended_master_secret: false,

            named_curve: NamedCurve::Unsupported,
            local_keypair: None,
            cookie: vec![],
            handshake_send_sequence: 0,
            handshake_recv_sequence: 0,
            local_verify_data: vec![], // cached VerifyData
        }
    }
}
