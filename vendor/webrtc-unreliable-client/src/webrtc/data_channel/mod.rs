pub(crate) mod data_channel_state;
pub(crate) mod internal;

use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, AtomicU8, AtomicUsize, Ordering};
use std::sync::{Arc, Weak};

use crate::webrtc::sctp::stream::OnBufferedAmountLowFn;
use async_lock::Mutex;

use data_channel_state::RTCDataChannelState;

use crate::webrtc::error::{Error, OnErrorHdlrFn, Result};
use crate::webrtc::sctp_transport::RTCSctpTransport;

pub(crate) type OnOpenHdlrFn =
    Box<dyn (FnOnce() -> Pin<Box<dyn Future<Output = ()> + Send + 'static>>) + Send + Sync>;

/// DataChannel represents a WebRTC DataChannel
/// The DataChannel interface represents a network channel
/// which can be used for bidirectional peer-to-peer transfers of arbitrary data
#[derive(Default)]
pub(crate) struct RTCDataChannel {
    label: String,
    protocol: String,

    ready_state: Arc<AtomicU8>, // DataChannelState
    buffered_amount_low_threshold: AtomicUsize,
    detach_called: Arc<AtomicBool>,

    // The binaryType represents attribute MUST, on getting, return the value to
    // which it was last set. On setting, if the new value is either the string
    // "blob" or the string "arraybuffer", then set the IDL attribute to this
    // new value. Otherwise, throw a SyntaxError. When an DataChannel object
    // is created, the binaryType attribute MUST be initialized to the string
    // "blob". This attribute controls how binary data is exposed to scripts.
    // binaryType                 string
    on_open_handler: Arc<Mutex<Option<OnOpenHdlrFn>>>,
    on_error_handler: Arc<Mutex<Option<OnErrorHdlrFn>>>,

    on_buffered_amount_low: Mutex<Option<OnBufferedAmountLowFn>>,

    sctp_transport: Mutex<Option<Weak<RTCSctpTransport>>>,
    data_channel: Mutex<Option<Arc<crate::webrtc::internal::data_channel::DataChannel>>>,
}

impl RTCDataChannel {
    // create the DataChannel object before the networking is set up.
    pub(crate) fn new(label: &str, protocol: &str) -> Self {
        RTCDataChannel {
            label: label.to_string(),
            protocol: protocol.to_string(),
            ready_state: Arc::new(AtomicU8::new(RTCDataChannelState::Connecting as u8)),
            detach_called: Arc::new(AtomicBool::new(false)),
            ..Default::default()
        }
    }

    /// open opens the datachannel over the sctp transport
    pub(crate) async fn open(&self, sctp_transport: Arc<RTCSctpTransport>) -> Result<()> {
        if let Some(association) = sctp_transport.association().await {
            {
                let mut st = self.sctp_transport.lock().await;
                if st.is_none() {
                    *st = Some(Arc::downgrade(&sctp_transport));
                } else {
                    return Ok(());
                }
            }

            let cfg = crate::webrtc::internal::data_channel::Config {
                label: self.label.clone(),
                protocol: self.protocol.clone(),
            };

            let dc = crate::webrtc::internal::data_channel::DataChannel::dial(&association, 0, cfg)
                .await?;

            // buffered_amount_low_threshold and on_buffered_amount_low might be set earlier
            dc.set_buffered_amount_low_threshold(
                self.buffered_amount_low_threshold.load(Ordering::SeqCst),
            );
            {
                let mut on_buffered_amount_low = self.on_buffered_amount_low.lock().await;
                if let Some(f) = on_buffered_amount_low.take() {
                    dc.on_buffered_amount_low(f).await;
                }
            }

            self.handle_open(Arc::new(dc)).await;

            Ok(())
        } else {
            Err(Error::ErrSCTPNotEstablished)
        }
    }

    /// on_open sets an event handler which is invoked when
    /// the underlying data transport has been established (or re-established).
    pub(crate) async fn on_open(&self, f: OnOpenHdlrFn) {
        {
            let mut handler = self.on_open_handler.lock().await;
            *handler = Some(f);
        }

        if self.ready_state() == RTCDataChannelState::Open {
            self.do_open().await;
        }
    }

    async fn do_open(&self) {
        let on_open_handler = Arc::clone(&self.on_open_handler);
        let detach_data_channels = true;
        let detach_called = Arc::clone(&self.detach_called);
        smol::spawn(async move {
            let mut handler = on_open_handler.lock().await;
            if let Some(f) = handler.take() {
                f().await;

                // self.check_detach_after_open();
                // After onOpen is complete check that the user called detach
                // and provide an error message if the call was missed
                if detach_data_channels && !detach_called.load(Ordering::SeqCst) {
                    log::warn!(
                        "webrtc.DetachDataChannels() enabled but didn't Detach, call Detach from OnOpen"
                    );
                }
            }
        })
        .detach();
    }

    pub(crate) async fn handle_open(
        &self,
        dc: Arc<crate::webrtc::internal::data_channel::DataChannel>,
    ) {
        {
            let mut data_channel = self.data_channel.lock().await;
            *data_channel = Some(Arc::clone(&dc));
        }
        self.set_ready_state(RTCDataChannelState::Open);

        self.do_open().await;
    }

    /// on_error sets an event handler which is invoked when
    /// the underlying data transport cannot be read.
    pub(crate) async fn on_error(&self, f: OnErrorHdlrFn) {
        let mut handler = self.on_error_handler.lock().await;
        *handler = Some(f);
    }

    /// detach allows you to detach the underlying datachannel. This provides
    /// an idiomatic API to work with, however it disables the OnMessage callback.
    /// Before calling Detach you have to enable this behavior by calling
    /// webrtc.DetachDataChannels(). Combining detached and normal data channels
    /// is not supported.
    /// Please refer to the data-channels-detach example and the
    /// pion/datachannel documentation for the correct way to handle the
    /// resulting DataChannel object.
    pub(crate) async fn detach(
        &self,
    ) -> Result<Arc<crate::webrtc::internal::data_channel::DataChannel>> {
        let data_channel = self.data_channel.lock().await;
        if let Some(dc) = &*data_channel {
            self.detach_called.store(true, Ordering::SeqCst);

            Ok(Arc::clone(dc))
        } else {
            Err(Error::ErrDetachBeforeOpened)
        }
    }

    /// ready_state represents the state of the DataChannel object.
    pub(crate) fn ready_state(&self) -> RTCDataChannelState {
        self.ready_state.load(Ordering::SeqCst).into()
    }

    pub(crate) fn set_ready_state(&self, r: RTCDataChannelState) {
        self.ready_state.store(r as u8, Ordering::SeqCst);
    }
}
