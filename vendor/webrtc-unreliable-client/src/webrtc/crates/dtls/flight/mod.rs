pub(crate) mod flight1;
pub(crate) mod flight3;
pub(crate) mod flight5;

use crate::webrtc::dtls::alert::*;
use crate::webrtc::dtls::error::Error;
use crate::webrtc::dtls::handshake::handshake_cache::*;
use crate::webrtc::dtls::handshaker::*;
use crate::webrtc::dtls::record_layer::*;
use crate::webrtc::dtls::state::*;

use async_trait::async_trait;
use smol::channel;
use std::fmt;

/*
  DTLS messages are grouped into a series of message flights, according
  to the diagrams below.  Although each Flight of messages may consist
  of a number of messages, they should be viewed as monolithic for the
  purpose of timeout and retransmission.
  https://tools.ietf.org/html/rfc4347#section-4.2.4
  Client                                          Server
  ------                                          ------
                                      Waiting                 Flight 0

  ClientHello             -------->                           Flight 1

                          <-------    HelloVerifyRequest      Flight 2

  ClientHello              -------->                           Flight 3

                                             ServerHello    \
                                            Certificate*     \
                                      ServerKeyExchange*      Flight 4
                                     CertificateRequest*     /
                          <--------      ServerHelloDone    /

  Certificate*                                              \
  ClientKeyExchange                                          \
  CertificateVerify*                                          Flight 5
  [ChangeCipherSpec]                                         /
  Finished                -------->                         /

                                      [ChangeCipherSpec]    \ Flight 6
                          <--------             Finished    /

*/

#[derive(Clone, Debug)]
pub(crate) struct Packet {
    pub(crate) record: RecordLayer,
    pub(crate) should_encrypt: bool,
}

#[async_trait]
pub(crate) trait Flight: fmt::Display + fmt::Debug {
    fn is_last_send_flight(&self) -> bool {
        false
    }
    fn is_last_recv_flight(&self) -> bool {
        false
    }
    fn has_retransmit(&self) -> bool {
        true
    }

    async fn parse(
        &self,
        tx: &mut channel::Sender<channel::Sender<()>>,
        state: &mut State,
        cache: &HandshakeCache,
        cfg: &HandshakeConfig,
    ) -> Result<Box<dyn Flight + Send + Sync>, (Option<Alert>, Option<Error>)>;

    async fn generate(
        &self,
        state: &mut State,
        cache: &HandshakeCache,
        cfg: &HandshakeConfig,
    ) -> Result<Vec<Packet>, (Option<Alert>, Option<Error>)>;
}
