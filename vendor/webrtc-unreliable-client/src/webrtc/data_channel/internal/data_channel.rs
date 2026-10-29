use crate::webrtc::data_channel::internal::error::Result;
use crate::webrtc::data_channel::internal::{message::message_channel_open::*, message::*};

use crate::webrtc::sctp::{
    association::Association, chunk::chunk_payload_data::PayloadProtocolIdentifier, stream::*,
};
use crate::webrtc::util::marshal::*;

use bytes::{Buf, Bytes};
use std::sync::Arc;

/// Config is used to configure the data channel.
#[derive(Eq, PartialEq, Default, Clone, Debug)]
pub(crate) struct Config {
    pub(crate) label: String,
    pub(crate) protocol: String,
}

/// DataChannel represents a data channel
#[derive(Debug, Default, Clone)]
pub(crate) struct DataChannel {
    stream: Arc<Stream>,
}

impl DataChannel {
    pub(crate) fn new(stream: Arc<Stream>) -> Self {
        Self { stream }
    }

    /// Dial opens a data channels over SCTP
    pub(crate) async fn dial(
        association: &Arc<Association>,
        identifier: u16,
        config: Config,
    ) -> Result<Self> {
        let stream = association.open_stream(identifier).await?;

        Self::client(stream, config).await
    }

    /// Client opens a data channel over an SCTP stream
    async fn client(stream: Arc<Stream>, config: Config) -> Result<Self> {
        let msg = Message::DataChannelOpen(DataChannelOpen {
            label: config.label.bytes().collect(),
            protocol: config.protocol.bytes().collect(),
        })
        .marshal()?;

        stream
            .write_sctp(&msg, PayloadProtocolIdentifier::Dcep)
            .await?;

        Ok(DataChannel::new(stream))
    }

    /// Read reads a packet of len(p) bytes as binary data
    pub(crate) async fn read(&self, buf: &mut [u8]) -> Result<usize> {
        self.read_data_channel(buf).await.map(|(n, _)| n)
    }

    /// ReadDataChannel reads a packet of len(p) bytes
    pub(crate) async fn read_data_channel(&self, buf: &mut [u8]) -> Result<(usize, bool)> {
        loop {
            //TODO: add handling of cancel read_data_channel
            let (mut n, ppi) = match self.stream.read_sctp(buf).await {
                Ok((n, ppi)) => (n, ppi),
                Err(err) => {
                    // When the peer sees that an incoming stream was
                    // reset, it also resets its corresponding outgoing stream.
                    self.stream.close().await?;

                    return Err(err.into());
                }
            };

            let mut is_string = false;
            match ppi {
                PayloadProtocolIdentifier::Dcep => {
                    let mut data = &buf[..n];
                    match self.handle_dcep(&mut data).await {
                        Ok(()) => {}
                        Err(err) => {
                            log::error!("Failed to handle DCEP: {:?}", err);
                        }
                    }
                    continue;
                }
                PayloadProtocolIdentifier::String | PayloadProtocolIdentifier::StringEmpty => {
                    is_string = true;
                }
                _ => {}
            };

            match ppi {
                PayloadProtocolIdentifier::StringEmpty | PayloadProtocolIdentifier::BinaryEmpty => {
                    n = 0;
                }
                _ => {}
            };

            return Ok((n, is_string));
        }
    }

    async fn handle_dcep<B>(&self, data: &mut B) -> Result<()>
    where
        B: Buf,
    {
        let msg = Message::unmarshal(data)?;

        match msg {
            Message::DataChannelAck(_) => {
                log::debug!("Received DATA_CHANNEL_ACK");
            }
            _ => {
                // Note: DATA_CHANNEL_OPEN message is handled inside Server() method.
                // Therefore, the message will not reach here.
                panic!("client should only ever receive a DataChannelAck message");
            }
        };

        Ok(())
    }

    /// Write writes len(p) bytes from p as binary data
    pub(crate) async fn write(&self, data: &Bytes) -> Result<usize> {
        self.write_data_channel(data, false).await
    }

    /// WriteDataChannel writes len(p) bytes from p
    pub(crate) async fn write_data_channel(&self, data: &Bytes, is_string: bool) -> Result<usize> {
        let data_len = data.len();

        // https://tools.ietf.org/html/draft-ietf-rtcweb-data-channel-12#section-6.6
        // SCTP does not support the sending of empty user messages.  Therefore,
        // if an empty message has to be sent, the appropriate PPID (WebRTC
        // String Empty or WebRTC Binary Empty) is used and the SCTP user
        // message of one zero byte is sent.  When receiving an SCTP user
        // message with one of these PPIDs, the receiver MUST ignore the SCTP
        // user message and process it as an empty message.
        let ppi = match (is_string, data_len) {
            (false, 0) => PayloadProtocolIdentifier::BinaryEmpty,
            (false, _) => PayloadProtocolIdentifier::Binary,
            (true, 0) => PayloadProtocolIdentifier::StringEmpty,
            (true, _) => PayloadProtocolIdentifier::String,
        };

        if data_len == 0 {
            let _ = self
                .stream
                .write_sctp(&Bytes::from_static(&[0]), ppi)
                .await?;
            Ok(0)
        } else {
            Ok(self.stream.write_sctp(data, ppi).await?)
        }
    }

    pub(crate) async fn close(&self) {
        self.stream.close().await.unwrap();
    }

    /// SetBufferedAmountLowThreshold is used to update the threshold.
    /// See BufferedAmountLowThreshold().
    pub(crate) fn set_buffered_amount_low_threshold(&self, threshold: usize) {
        self.stream.set_buffered_amount_low_threshold(threshold)
    }

    /// OnBufferedAmountLow sets the callback handler which would be called when the
    /// number of bytes of outgoing data buffered is lower than the threshold.
    pub(crate) async fn on_buffered_amount_low(&self, f: OnBufferedAmountLowFn) {
        self.stream.on_buffered_amount_low(f).await
    }
}
