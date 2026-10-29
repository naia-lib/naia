pub(crate) mod sctp_transport_state;

use sctp_transport_state::RTCSctpTransportState;

use crate::webrtc::data_channel::RTCDataChannel;
use crate::webrtc::dtls_transport::*;
use crate::webrtc::error::*;
use crate::webrtc::sctp::association::Association;
use crate::webrtc::util::Conn;

use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU8, Ordering};
use std::sync::Arc;

use async_lock::Mutex;

pub(crate) type OnDataChannelHdlrFn = Box<
    dyn (FnMut(Arc<RTCDataChannel>) -> Pin<Box<dyn Future<Output = ()> + Send + 'static>>)
        + Send
        + Sync,
>;

/// SCTPTransport provides details about the SCTP transport.
#[derive(Default)]
pub(crate) struct RTCSctpTransport {
    // removing this causes compile panic, last checked
    #[allow(dead_code)]
    max_message_size: bool,
    #[allow(dead_code)]
    setting_engine: bool,

    pub(crate) dtls_transport: Arc<RTCDtlsTransport>,

    // State represents the current state of the SCTP transport.
    state: AtomicU8, //SCTPTransportState,

    // SCTPTransportState doesn't have an enum to distinguish between New/Connecting
    // so we need a dedicated field
    is_started: AtomicBool,

    sctp_association: Mutex<Option<Arc<Association>>>,

    on_data_channel_handler: Arc<Mutex<Option<OnDataChannelHdlrFn>>>,

    // DataChannels
    pub(crate) data_channels: Arc<Mutex<Vec<Arc<RTCDataChannel>>>>,
    pub(crate) data_channels_opened: Arc<AtomicU32>,
    pub(crate) data_channels_requested: Arc<AtomicU32>,
}

impl RTCSctpTransport {
    pub(crate) fn new(dtls_transport: Arc<RTCDtlsTransport>) -> Self {
        RTCSctpTransport {
            setting_engine: true,
            max_message_size: true,

            dtls_transport,
            state: AtomicU8::new(RTCSctpTransportState::Connecting as u8),
            is_started: AtomicBool::new(false),
            sctp_association: Mutex::new(None),
            on_data_channel_handler: Arc::new(Mutex::new(None)),
            data_channels: Arc::new(Mutex::new(vec![])),
            data_channels_opened: Arc::new(AtomicU32::new(0)),
            data_channels_requested: Arc::new(AtomicU32::new(0)),
        }
    }

    /// transport returns the DTLSTransport instance the SCTPTransport is sending over.
    pub(crate) fn transport(&self) -> Arc<RTCDtlsTransport> {
        Arc::clone(&self.dtls_transport)
    }

    /// Start the SCTPTransport. Since both local and remote parties must mutually
    /// create an SCTPTransport, SCTP SO (Simultaneous Open) is used to establish
    /// a connection over SCTP.
    pub(crate) async fn start(&self) -> Result<()> {
        if self.is_started.load(Ordering::SeqCst) {
            return Ok(());
        }
        self.is_started.store(true, Ordering::SeqCst);

        let dtls_transport = self.transport();
        if let Some(net_conn) = &dtls_transport.conn().await {
            let sctp_association = Arc::new(
                crate::webrtc::sctp::association::Association::client(
                    crate::webrtc::sctp::association::Config {
                        net_conn: Arc::clone(net_conn) as Arc<dyn Conn + Send + Sync>,
                        max_receive_buffer_size: 0,
                        max_message_size: 0,
                        name: String::new(),
                    },
                )
                .await?,
            );

            {
                let mut sa = self.sctp_association.lock().await;
                *sa = Some(Arc::clone(&sctp_association));
            }
            self.state
                .store(RTCSctpTransportState::Connected as u8, Ordering::SeqCst);

            Ok(())
        } else {
            Err(Error::ErrSCTPTransportDTLS)
        }
    }

    /// Stop stops the SCTPTransport
    pub(crate) async fn stop(&self) -> Result<()> {
        {
            let mut sctp_association = self.sctp_association.lock().await;
            if let Some(sa) = sctp_association.take() {
                sa.close().await?;
            }
        }

        self.state
            .store(RTCSctpTransportState::Closed as u8, Ordering::SeqCst);

        Ok(())
    }

    /// on_data_channel sets an event handler which is invoked when a data
    /// channel message arrives from a remote peer.
    pub(crate) async fn on_data_channel(&self, f: OnDataChannelHdlrFn) {
        let mut handler = self.on_data_channel_handler.lock().await;
        *handler = Some(f);
    }

    /// state returns the current state of the SCTPTransport
    pub(crate) fn state(&self) -> RTCSctpTransportState {
        self.state.load(Ordering::SeqCst).into()
    }

    pub(crate) async fn association(&self) -> Option<Arc<Association>> {
        let sctp_association = self.sctp_association.lock().await;
        sctp_association.clone()
    }
}
