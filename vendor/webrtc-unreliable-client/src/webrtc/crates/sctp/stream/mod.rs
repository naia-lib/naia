use crate::webrtc::sctp::association::AssociationState;
use crate::webrtc::sctp::chunk::chunk_payload_data::{ChunkPayloadData, PayloadProtocolIdentifier};
use crate::webrtc::sctp::error::{Error, Result};
use crate::webrtc::sctp::queue::reassembly_queue::ReassemblyQueue;

use crate::webrtc::sctp::queue::pending_queue::PendingQueue;

use super::super::util::notify::Notify;
use async_lock::Mutex;
use bytes::Bytes;
use smol::channel;
use std::fmt;
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, AtomicU16, AtomicU32, AtomicU8, AtomicUsize, Ordering};
use std::sync::Arc;

pub(crate) type OnBufferedAmountLowFn =
    Box<dyn (FnMut() -> Pin<Box<dyn Future<Output = ()> + Send + 'static>>) + Send + Sync>;

// TODO: benchmark performance between multiple Atomic+Mutex vs one Mutex<StreamInternal>

/// Stream represents an SCTP stream
#[derive(Default)]
pub(crate) struct Stream {
    pub(crate) max_payload_size: u32,
    pub(crate) max_message_size: Arc<AtomicU32>, // clone from association
    pub(crate) state: Arc<AtomicU8>,             // clone from association
    pub(crate) awake_write_loop_ch: Option<Arc<channel::Sender<()>>>,
    pub(crate) pending_queue: Arc<PendingQueue>,

    pub(crate) stream_identifier: u16,
    pub(crate) reassembly_queue: Mutex<ReassemblyQueue>,
    pub(crate) sequence_number: AtomicU16,
    pub(crate) read_notifier: Notify,
    pub(crate) closed: AtomicBool,
    pub(crate) buffered_amount: AtomicUsize,
    pub(crate) buffered_amount_low: AtomicUsize,
    pub(crate) on_buffered_amount_low: Mutex<Option<OnBufferedAmountLowFn>>,
    pub(crate) name: String,
}

impl fmt::Debug for Stream {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Stream")
            .field("max_payload_size", &self.max_payload_size)
            .field("max_message_size", &self.max_message_size)
            .field("state", &self.state)
            .field("awake_write_loop_ch", &self.awake_write_loop_ch)
            .field("stream_identifier", &self.stream_identifier)
            .field("reassembly_queue", &self.reassembly_queue)
            .field("sequence_number", &self.sequence_number)
            .field("closed", &self.closed)
            .field("buffered_amount", &self.buffered_amount)
            .field("buffered_amount_low", &self.buffered_amount_low)
            .field("name", &self.name)
            .finish()
    }
}

impl Stream {
    pub(crate) fn new(
        name: String,
        stream_identifier: u16,
        max_payload_size: u32,
        max_message_size: Arc<AtomicU32>,
        state: Arc<AtomicU8>,
        awake_write_loop_ch: Option<Arc<channel::Sender<()>>>,
        pending_queue: Arc<PendingQueue>,
    ) -> Self {
        Stream {
            max_payload_size,
            max_message_size,
            state,
            awake_write_loop_ch,
            pending_queue,

            stream_identifier,
            reassembly_queue: Mutex::new(ReassemblyQueue::new(stream_identifier)),
            sequence_number: AtomicU16::new(0),
            read_notifier: Notify::new(),
            closed: AtomicBool::new(false),
            buffered_amount: AtomicUsize::new(0),
            buffered_amount_low: AtomicUsize::new(0),
            on_buffered_amount_low: Mutex::new(None),
            name,
        }
    }

    /// read_sctp reads a packet of len(p) bytes and returns the associated Payload
    /// Protocol Identifier.
    /// Returns EOF when the stream is reset or an error if the stream is closed
    /// otherwise.
    pub(crate) async fn read_sctp(
        &self,
        p: &mut [u8],
    ) -> Result<(usize, PayloadProtocolIdentifier)> {
        while !self.closed.load(Ordering::SeqCst) {
            let result = {
                let mut reassembly_queue = self.reassembly_queue.lock().await;
                reassembly_queue.read(p)
            };

            if result.is_ok() {
                return result;
            } else if let Err(err) = result {
                if Error::ErrShortBuffer == err {
                    return Err(err);
                }
            }

            self.read_notifier.notified().await;
        }

        Err(Error::ErrStreamClosed)
    }

    pub(crate) async fn handle_data(&self, pd: ChunkPayloadData) {
        let readable = {
            let mut reassembly_queue = self.reassembly_queue.lock().await;
            if reassembly_queue.push(pd) {
                let readable = reassembly_queue.is_readable();
                log::debug!("[{}] reassemblyQueue readable={}", self.name, readable);
                readable
            } else {
                false
            }
        };

        if readable {
            log::debug!("[{}] readNotifier.signal()", self.name);
            self.read_notifier.notify_one();
            log::debug!("[{}] readNotifier.signal() done", self.name);
        }
    }

    pub(crate) async fn handle_forward_tsn_for_unordered(&self, new_cumulative_tsn: u32) {
        // Remove all chunks older than or equal to the new TSN from
        // the reassembly_queue.
        let readable = {
            let mut reassembly_queue = self.reassembly_queue.lock().await;
            reassembly_queue.forward_tsn_for_unordered(new_cumulative_tsn);
            reassembly_queue.is_readable()
        };

        // Notify the reader asynchronously if there's a data chunk to read.
        if readable {
            self.read_notifier.notify_one();
        }
    }

    /// write_sctp writes len(p) bytes from p to the DTLS connection
    pub(crate) async fn write_sctp(
        &self,
        p: &Bytes,
        ppi: PayloadProtocolIdentifier,
    ) -> Result<usize> {
        if p.len() > self.max_message_size.load(Ordering::SeqCst) as usize {
            return Err(Error::ErrOutboundPacketTooLarge);
        }

        let state: AssociationState = self.state.load(Ordering::SeqCst).into();
        match state {
            AssociationState::ShutdownSent
            | AssociationState::ShutdownAckSent
            | AssociationState::ShutdownPending
            | AssociationState::ShutdownReceived => return Err(Error::ErrStreamClosed),
            _ => {}
        };

        let chunks = self.packetize(p, ppi);
        self.send_payload_data(chunks).await?;

        Ok(p.len())
    }

    fn packetize(&self, raw: &Bytes, ppi: PayloadProtocolIdentifier) -> Vec<ChunkPayloadData> {
        let mut i = 0;
        let mut remaining = raw.len();

        // From draft-ietf-rtcweb-data-protocol-09, section 6:
        //   All Data Channel Establishment Protocol messages MUST be sent using
        //   ordered delivery and reliable transmission.
        let unordered = ppi != PayloadProtocolIdentifier::Dcep;

        let mut chunks = vec![];

        let head_abandoned = Arc::new(AtomicBool::new(false));
        let head_all_inflight = Arc::new(AtomicBool::new(false));
        while remaining != 0 {
            let fragment_size = std::cmp::min(self.max_payload_size as usize, remaining); //self.association.max_payload_size

            // Copy the userdata since we'll have to store it until acked
            // and the caller may re-use the buffer in the mean time
            let user_data = raw.slice(i..i + fragment_size);

            let chunk = ChunkPayloadData {
                stream_identifier: self.stream_identifier,
                user_data,
                unordered,
                beginning_fragment: i == 0,
                ending_fragment: remaining - fragment_size == 0,
                immediate_sack: false,
                payload_type: ppi,
                stream_sequence_number: self.sequence_number.load(Ordering::SeqCst),
                abandoned: head_abandoned.clone(), // all fragmented chunks use the same abandoned
                all_inflight: head_all_inflight.clone(), // all fragmented chunks use the same all_inflight
                ..Default::default()
            };

            chunks.push(chunk);

            remaining -= fragment_size;
            i += fragment_size;
        }

        // RFC 4960 Sec 6.6
        // Note: When transmitting ordered and unordered data, an endpoint does
        // not increment its Stream Sequence Number when transmitting a DATA
        // chunk with U flag set to 1.
        if !unordered {
            self.sequence_number.fetch_add(1, Ordering::SeqCst);
        }

        let old_value = self.buffered_amount.fetch_add(raw.len(), Ordering::SeqCst);
        log::trace!("[{}] bufferedAmount = {}", self.name, old_value + raw.len());

        chunks
    }

    /// Close closes the write-direction of the stream.
    /// Future calls to write are not permitted after calling Close.
    pub(crate) async fn close(&self) -> Result<()> {
        if !self.closed.load(Ordering::SeqCst) {
            // Reset the outgoing stream
            // https://tools.ietf.org/html/rfc6525
            self.send_reset_request(self.stream_identifier).await?;
        }
        self.closed.store(true, Ordering::SeqCst);
        self.read_notifier.notify_waiters(); // broadcast regardless

        Ok(())
    }

    /// set_buffered_amount_low_threshold is used to update the threshold.
    /// See buffered_amount_low_threshold().
    pub(crate) fn set_buffered_amount_low_threshold(&self, th: usize) {
        self.buffered_amount_low.store(th, Ordering::SeqCst);
    }

    /// on_buffered_amount_low sets the callback handler which would be called when the number of
    /// bytes of outgoing data buffered is lower than the threshold.
    pub(crate) async fn on_buffered_amount_low(&self, f: OnBufferedAmountLowFn) {
        let mut on_buffered_amount_low = self.on_buffered_amount_low.lock().await;
        *on_buffered_amount_low = Some(f);
    }

    /// This method is called by association's read_loop (go-)routine to notify this stream
    /// of the specified amount of outgoing data has been delivered to the peer.
    pub(crate) async fn on_buffer_released(&self, n_bytes_released: i64) {
        if n_bytes_released <= 0 {
            return;
        }

        let from_amount = self.buffered_amount.load(Ordering::SeqCst);
        let new_amount = if from_amount < n_bytes_released as usize {
            self.buffered_amount.store(0, Ordering::SeqCst);
            log::error!(
                "[{}] released buffer size {} should be <= {}",
                self.name,
                n_bytes_released,
                0,
            );
            0
        } else {
            self.buffered_amount
                .fetch_sub(n_bytes_released as usize, Ordering::SeqCst);

            from_amount - n_bytes_released as usize
        };

        let buffered_amount_low = self.buffered_amount_low.load(Ordering::SeqCst);

        log::trace!(
            "[{}] bufferedAmount = {}, from_amount = {}, buffered_amount_low = {}",
            self.name,
            new_amount,
            from_amount,
            buffered_amount_low,
        );

        if from_amount > buffered_amount_low && new_amount <= buffered_amount_low {
            let mut handler = self.on_buffered_amount_low.lock().await;
            if let Some(f) = &mut *handler {
                f().await;
            }
        }
    }

    /// get_num_bytes_in_reassembly_queue returns the number of bytes of data currently queued to
    /// be read (once chunk is complete).
    pub(crate) async fn get_num_bytes_in_reassembly_queue(&self) -> usize {
        // No lock is required as it reads the size with atomic load function.
        let reassembly_queue = self.reassembly_queue.lock().await;
        reassembly_queue.get_num_bytes()
    }

    /// get_state atomically returns the state of the Association.
    fn get_state(&self) -> AssociationState {
        self.state.load(Ordering::SeqCst).into()
    }

    fn awake_write_loop(&self) {
        //log::debug!("[{}] awake_write_loop_ch.notify_one", self.name);
        if let Some(awake_write_loop_ch) = &self.awake_write_loop_ch {
            let _ = awake_write_loop_ch.try_send(());
        }
    }

    async fn send_payload_data(&self, chunks: Vec<ChunkPayloadData>) -> Result<()> {
        let state = self.get_state();
        if state != AssociationState::Established {
            return Err(Error::ErrPayloadDataStateNotExist);
        }

        // Push the chunks into the pending queue first.
        for c in chunks {
            self.pending_queue.push(c).await;
        }

        self.awake_write_loop();
        Ok(())
    }

    async fn send_reset_request(&self, stream_identifier: u16) -> Result<()> {
        let state = self.get_state();
        if state != AssociationState::Established {
            return Err(Error::ErrResetPacketInStateNotExist);
        }

        // Create DATA chunk which only contains valid stream identifier with
        // nil userData and use it as a EOS from the stream.
        let c = ChunkPayloadData {
            stream_identifier,
            beginning_fragment: true,
            ending_fragment: true,
            user_data: Bytes::new(),
            ..Default::default()
        };

        self.pending_queue.push(c).await;

        self.awake_write_loop();
        Ok(())
    }
}
