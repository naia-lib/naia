use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::Arc;

use crate::webrtc::dtls::conn::DTLSConn;
use crate::webrtc::util::Conn;
use async_lock::Mutex;

use dtls_role::*;

use crate::webrtc::dtls_transport::dtls_transport_state::RTCDtlsTransportState;
use crate::webrtc::error::{Error, Result};
use crate::webrtc::ice_transport::ice_transport_state::RTCIceTransportState;
use crate::webrtc::ice_transport::RTCIceTransport;
use crate::webrtc::mux::mux_func::match_dtls;
use crate::webrtc::peer_connection::certificate::RTCCertificate;

pub(crate) mod dtls_fingerprint;
pub(crate) mod dtls_role;
pub(crate) mod dtls_transport_state;

pub(crate) type OnDTLSTransportStateChangeHdlrFn = Box<
    dyn (FnMut(RTCDtlsTransportState) -> Pin<Box<dyn Future<Output = ()> + Send + 'static>>)
        + Send
        + Sync,
>;

/// DTLSTransport allows an application access to information about the DTLS
/// transport over which RTP and RTCP packets are sent and received by
/// RTPSender and RTPReceiver, as well other data such as SCTP packets sent
/// and received by data channels.
#[derive(Default)]
pub(crate) struct RTCDtlsTransport {
    pub(crate) ice_transport: Arc<RTCIceTransport>,
    pub(crate) certificates: Vec<RTCCertificate>,

    pub(crate) state: AtomicU8, //DTLSTransportState,
    pub(crate) on_state_change_handler: Arc<Mutex<Option<OnDTLSTransportStateChangeHdlrFn>>>,
    pub(crate) conn: Mutex<Option<Arc<DTLSConn>>>,
}

impl RTCDtlsTransport {
    pub(crate) fn new(
        ice_transport: Arc<RTCIceTransport>,
        certificates: Vec<RTCCertificate>,
    ) -> Self {
        RTCDtlsTransport {
            ice_transport,
            certificates,
            state: AtomicU8::new(RTCDtlsTransportState::New as u8),
            ..Default::default()
        }
    }

    pub(crate) async fn conn(&self) -> Option<Arc<DTLSConn>> {
        let conn = self.conn.lock().await;
        conn.clone()
    }

    /// state_change requires the caller holds the lock
    async fn state_change(&self, state: RTCDtlsTransportState) {
        self.state.store(state as u8, Ordering::SeqCst);
        let mut handler = self.on_state_change_handler.lock().await;
        if let Some(f) = &mut *handler {
            f(state).await;
        }
    }

    /// state returns the current dtls_transport transport state.
    pub(crate) fn state(&self) -> RTCDtlsTransportState {
        self.state.load(Ordering::SeqCst).into()
    }

    async fn prepare_transport(&self) -> Result<(DTLSRole, crate::webrtc::dtls::config::Config)> {
        self.ensure_ice_conn()?;

        if self.state() != RTCDtlsTransportState::New {
            return Err(Error::ErrInvalidDTLSStart);
        }

        let certificate = if let Some(cert) = self.certificates.first() {
            cert.certificate.clone()
        } else {
            return Err(Error::ErrNonCertificate);
        };
        self.state_change(RTCDtlsTransportState::Connecting).await;

        Ok((
            DTLSRole::Client,
            crate::webrtc::dtls::config::Config {
                certificates: vec![certificate],
                ..Default::default()
            },
        ))
    }

    /// start DTLS transport negotiation with the parameters of the remote DTLS transport
    pub(crate) async fn start(&self) -> Result<()> {
        let dtls_conn_result = if let Some(dtls_endpoint) =
            self.ice_transport.new_endpoint(Box::new(match_dtls)).await
        {
            let (_, dtls_config) = self.prepare_transport().await?;

            // Connect as DTLS Client/Server, function is blocking and we
            // must not hold the DTLSTransport lock
            crate::webrtc::dtls::conn::DTLSConn::new(
                dtls_endpoint as Arc<dyn Conn + Send + Sync>,
                dtls_config,
                true,
            )
            .await
        } else {
            Err(crate::webrtc::dtls::Error::Other(
                "ice_transport.new_endpoint failed".to_owned(),
            ))
        };

        let dtls_conn = match dtls_conn_result {
            Ok(dtls_conn) => dtls_conn,
            Err(err) => {
                self.state_change(RTCDtlsTransportState::Failed).await;
                return Err(err.into());
            }
        };

        {
            let mut conn = self.conn.lock().await;
            *conn = Some(Arc::new(dtls_conn));
        }
        self.state_change(RTCDtlsTransportState::Connected).await;

        Ok(())
    }

    pub(crate) fn ensure_ice_conn(&self) -> Result<()> {
        if self.ice_transport.state() == RTCIceTransportState::New {
            Err(Error::ErrICEConnectionNotStarted)
        } else {
            Ok(())
        }
    }
}
