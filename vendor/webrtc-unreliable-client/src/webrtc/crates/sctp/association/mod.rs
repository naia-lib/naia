mod association_internal;
mod association_stats;

use crate::webrtc::sctp::chunk::chunk_abort::ChunkAbort;
use crate::webrtc::sctp::chunk::chunk_cookie_ack::ChunkCookieAck;
use crate::webrtc::sctp::chunk::chunk_cookie_echo::ChunkCookieEcho;
use crate::webrtc::sctp::chunk::chunk_error::ChunkError;
use crate::webrtc::sctp::chunk::chunk_forward_tsn::{ChunkForwardTsn, ChunkForwardTsnStream};
use crate::webrtc::sctp::chunk::chunk_heartbeat::ChunkHeartbeat;
use crate::webrtc::sctp::chunk::chunk_heartbeat_ack::ChunkHeartbeatAck;
use crate::webrtc::sctp::chunk::chunk_init::ChunkInit;
use crate::webrtc::sctp::chunk::chunk_payload_data::{ChunkPayloadData, PayloadProtocolIdentifier};
use crate::webrtc::sctp::chunk::chunk_reconfig::ChunkReconfig;
use crate::webrtc::sctp::chunk::chunk_selective_ack::ChunkSelectiveAck;
use crate::webrtc::sctp::chunk::chunk_shutdown::ChunkShutdown;
use crate::webrtc::sctp::chunk::chunk_shutdown_ack::ChunkShutdownAck;
use crate::webrtc::sctp::chunk::chunk_shutdown_complete::ChunkShutdownComplete;
use crate::webrtc::sctp::chunk::chunk_type::*;
use crate::webrtc::sctp::chunk::Chunk;
use crate::webrtc::sctp::error::{Error, Result};
use crate::webrtc::sctp::error_cause::*;
use crate::webrtc::sctp::packet::Packet;
use crate::webrtc::sctp::param::param_heartbeat_info::ParamHeartbeatInfo;
use crate::webrtc::sctp::param::param_outgoing_reset_request::ParamOutgoingResetRequest;
use crate::webrtc::sctp::param::param_reconfig_response::{ParamReconfigResponse, ReconfigResult};
use crate::webrtc::sctp::param::param_state_cookie::ParamStateCookie;
use crate::webrtc::sctp::param::param_supported_extensions::ParamSupportedExtensions;
use crate::webrtc::sctp::param::Param;
use crate::webrtc::sctp::queue::control_queue::ControlQueue;
use crate::webrtc::sctp::queue::payload_queue::PayloadQueue;
use crate::webrtc::sctp::queue::pending_queue::PendingQueue;
use crate::webrtc::sctp::stream::*;
use crate::webrtc::sctp::timer::ack_timer::*;
use crate::webrtc::sctp::timer::rtx_timer::*;
use crate::webrtc::sctp::util::*;

use association_internal::*;
use association_stats::*;

use crate::webrtc::util::Conn;
use async_lock::Mutex;
use bytes::Bytes;
use futures_util::{select, FutureExt};
use rand::random;
use smol::channel;
use std::collections::{HashMap, VecDeque};
use std::fmt;
use std::sync::atomic::{AtomicU32, AtomicU8, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::SystemTime;

pub(crate) const RECEIVE_MTU: usize = 8192;
/// MTU for inbound packet (from DTLS)
pub(crate) const INITIAL_MTU: u32 = 1228;
/// initial MTU for outgoing packets (to DTLS)
pub(crate) const INITIAL_RECV_BUF_SIZE: u32 = 1024 * 1024;
pub(crate) const COMMON_HEADER_SIZE: u32 = 12;
pub(crate) const DATA_CHUNK_HEADER_SIZE: u32 = 16;
pub(crate) const DEFAULT_MAX_MESSAGE_SIZE: u32 = 65536;

/// other constants
pub(crate) const ACCEPT_CH_SIZE: usize = 16;

/// association state enums
#[derive(Debug, Copy, Clone, PartialEq)]
pub(crate) enum AssociationState {
    Closed = 0,
    CookieWait = 1,
    CookieEchoed = 2,
    Established = 3,
    ShutdownAckSent = 4,
    ShutdownPending = 5,
    ShutdownReceived = 6,
    ShutdownSent = 7,
}

impl From<u8> for AssociationState {
    fn from(v: u8) -> AssociationState {
        match v {
            1 => AssociationState::CookieWait,
            2 => AssociationState::CookieEchoed,
            3 => AssociationState::Established,
            4 => AssociationState::ShutdownAckSent,
            5 => AssociationState::ShutdownPending,
            6 => AssociationState::ShutdownReceived,
            7 => AssociationState::ShutdownSent,
            _ => AssociationState::Closed,
        }
    }
}

impl fmt::Display for AssociationState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match *self {
            AssociationState::Closed => "Closed",
            AssociationState::CookieWait => "CookieWait",
            AssociationState::CookieEchoed => "CookieEchoed",
            AssociationState::Established => "Established",
            AssociationState::ShutdownPending => "ShutdownPending",
            AssociationState::ShutdownSent => "ShutdownSent",
            AssociationState::ShutdownReceived => "ShutdownReceived",
            AssociationState::ShutdownAckSent => "ShutdownAckSent",
        };
        write!(f, "{}", s)
    }
}

/// retransmission timer IDs
#[derive(Debug, Copy, Clone, PartialEq, Default)]
pub(crate) enum RtxTimerId {
    #[default]
    T1Init,
    T1Cookie,
    T2Shutdown,
    T3RTX,
    Reconfig,
}

impl fmt::Display for RtxTimerId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match *self {
            RtxTimerId::T1Init => "T1Init",
            RtxTimerId::T1Cookie => "T1Cookie",
            RtxTimerId::T2Shutdown => "T2Shutdown",
            RtxTimerId::T3RTX => "T3RTX",
            RtxTimerId::Reconfig => "Reconfig",
        };
        write!(f, "{}", s)
    }
}

/// ack mode (for testing)
#[derive(Debug, Copy, Clone, PartialEq, Default)]
pub(crate) enum AckMode {
    #[default]
    Normal,
    AlwaysDelay,
}

impl fmt::Display for AckMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match *self {
            AckMode::Normal => "Normal",
            AckMode::AlwaysDelay => "AlwaysDelay",
        };
        write!(f, "{}", s)
    }
}

/// ack transmission state
#[derive(Debug, Copy, Clone, PartialEq, Default)]
pub(crate) enum AckState {
    #[default]
    Idle, // ack timer is off
    Immediate, // will send ack immediately
    Delay,     // ack timer is on (ack is being delayed)
}

impl fmt::Display for AckState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match *self {
            AckState::Idle => "Idle",
            AckState::Immediate => "Immediate",
            AckState::Delay => "Delay",
        };
        write!(f, "{}", s)
    }
}

/// Config collects the arguments to create_association construction into
/// a single structure
pub(crate) struct Config {
    pub(crate) net_conn: Arc<dyn Conn + Send + Sync>,
    pub(crate) max_receive_buffer_size: u32,
    pub(crate) max_message_size: u32,
    pub(crate) name: String,
}

///Association represents an SCTP association
///13.2.  Parameters Necessary per Association (i.e., the TCB)
///Peer : Tag value to be sent in every packet and is received
///Verification: in the INIT or INIT ACK chunk.
///Tag :
///
///My : Tag expected in every inbound packet and sent in the
///Verification: INIT or INIT ACK chunk.
///
///Tag :
///State : A state variable indicating what state the association
/// : is in, i.e., COOKIE-WAIT, COOKIE-ECHOED, ESTABLISHED,
/// : SHUTDOWN-PENDING, SHUTDOWN-SENT, SHUTDOWN-RECEIVED,
/// : SHUTDOWN-ACK-SENT.
///
/// No Closed state is illustrated since if a
/// association is Closed its TCB SHOULD be removed.
pub(crate) struct Association {
    name: String,
    net_conn: Arc<dyn Conn + Send + Sync>,

    pub(crate) association_internal: Arc<Mutex<AssociationInternal>>,
}

impl Association {
    /// Client opens a SCTP stream over a conn
    pub(crate) async fn client(config: Config) -> Result<Self> {
        let (a, handshake_completed_ch_rx) = Association::new(config, true).await?;

        if let Ok(err_opt) = handshake_completed_ch_rx.recv().await {
            if let Some(err) = err_opt {
                Err(err)
            } else {
                Ok(a)
            }
        } else {
            Err(Error::ErrAssociationHandshakeClosed)
        }
    }

    /// Close ends the SCTP Association and cleans up any state
    pub(crate) async fn close(&self) -> Result<()> {
        log::debug!("[{}] closing association..", self.name);

        let _ = self.net_conn.close().await;

        let mut ai = self.association_internal.lock().await;
        ai.close().await
    }

    async fn new(
        config: Config,
        is_client: bool,
    ) -> Result<(Self, channel::Receiver<Option<Error>>)> {
        let net_conn = Arc::clone(&config.net_conn);

        let (awake_write_loop_ch_tx, awake_write_loop_ch_rx) = channel::bounded(1);
        let (accept_ch_tx, _accept_ch_rx) = channel::bounded(ACCEPT_CH_SIZE);
        let (handshake_completed_ch_tx, handshake_completed_ch_rx) = channel::bounded(1);
        // NOTE: the original receiver must stay alive: dropping it closes the
        // channel (async-broadcast closes when receiver count hits zero and
        // new_receiver() will not re-open it), which would make every recv()
        // return Err(Closed) instantly and kill both loops on first poll.
        // (tokio broadcast::channel did not close on receiver drop.)
        let (close_loop_ch_tx, close_loop_ch_rx1) = async_broadcast::broadcast(1);
        let close_loop_ch_rx2 = close_loop_ch_tx.new_receiver();
        let awake_write_loop_ch = Arc::new(awake_write_loop_ch_tx);

        let ai = AssociationInternal::new(
            config,
            close_loop_ch_tx,
            accept_ch_tx,
            handshake_completed_ch_tx,
            Arc::clone(&awake_write_loop_ch),
        );

        let bytes_received = Arc::new(AtomicUsize::new(0));
        let bytes_sent = Arc::new(AtomicUsize::new(0));
        let name = ai.name.clone();

        let mut init = ChunkInit {
            initial_tsn: ai.my_next_tsn,
            num_outbound_streams: ai.my_max_num_outbound_streams,
            num_inbound_streams: ai.my_max_num_inbound_streams,
            initiate_tag: ai.my_verification_tag,
            advertised_receiver_window_credit: ai.max_receive_buffer_size,
            ..Default::default()
        };
        init.set_forward_tsn_supported();

        let name1 = name.clone();
        let name2 = name.clone();

        let bytes_received1 = Arc::clone(&bytes_received);
        let bytes_sent2 = Arc::clone(&bytes_sent);

        let net_conn1 = Arc::clone(&net_conn);
        let net_conn2 = Arc::clone(&net_conn);

        let association_internal = Arc::new(Mutex::new(ai));
        let association_internal1 = Arc::clone(&association_internal);
        let association_internal2 = Arc::clone(&association_internal);

        {
            let association_internal3 = Arc::clone(&association_internal);

            let mut ai = association_internal.lock().await;
            ai.t1init = Some(RtxTimer::new(
                Arc::downgrade(&association_internal3),
                RtxTimerId::T1Init,
                MAX_INIT_RETRANS,
            ));
            ai.t1cookie = Some(RtxTimer::new(
                Arc::downgrade(&association_internal3),
                RtxTimerId::T1Cookie,
                MAX_INIT_RETRANS,
            ));
            ai.t2shutdown = Some(RtxTimer::new(
                Arc::downgrade(&association_internal3),
                RtxTimerId::T2Shutdown,
                NO_MAX_RETRANS,
            )); // retransmit forever
            ai.t3rtx = Some(RtxTimer::new(
                Arc::downgrade(&association_internal3),
                RtxTimerId::T3RTX,
                NO_MAX_RETRANS,
            )); // retransmit forever
            ai.treconfig = Some(RtxTimer::new(
                Arc::downgrade(&association_internal3),
                RtxTimerId::Reconfig,
                NO_MAX_RETRANS,
            )); // retransmit forever
            ai.ack_timer = Some(AckTimer::new(
                Arc::downgrade(&association_internal3),
                ACK_INTERVAL,
            ));
        }

        smol::spawn(async move {
            Association::read_loop(
                name1,
                bytes_received1,
                net_conn1,
                close_loop_ch_rx1,
                association_internal1,
            )
            .await;
        })
        .detach();

        smol::spawn(async move {
            Association::write_loop(
                name2,
                bytes_sent2,
                net_conn2,
                close_loop_ch_rx2,
                association_internal2,
                awake_write_loop_ch_rx,
            )
            .await;
        })
        .detach();

        if is_client {
            let mut ai = association_internal.lock().await;
            ai.set_state(AssociationState::CookieWait);
            ai.stored_init = Some(init);
            ai.send_init()?;
            let rto = ai.rto_mgr.get_rto();
            if let Some(t1init) = &ai.t1init {
                t1init.start(rto).await;
            }
        }

        Ok((
            Association {
                name,
                net_conn,
                association_internal,
            },
            handshake_completed_ch_rx,
        ))
    }

    async fn read_loop(
        name: String,
        bytes_received: Arc<AtomicUsize>,
        net_conn: Arc<dyn Conn + Send + Sync>,
        mut close_loop_ch: async_broadcast::Receiver<()>,
        association_internal: Arc<Mutex<AssociationInternal>>,
    ) {
        log::debug!("[{}] read_loop entered", name);

        let mut buffer = vec![0u8; RECEIVE_MTU];
        let mut done = false;
        let mut n;
        while !done {
            select! {
                _ = close_loop_ch.recv().fuse() => {
                    break;
                }
                result = net_conn.recv(&mut buffer).fuse() => {
                    match result {
                        Ok(m) => {
                            n=m;
                        }
                        Err(err) => {
                            log::warn!("[{}] failed to read packets on net_conn: {}", name, err);
                            break;
                        }
                    }
                }
            };

            // Make a buffer sized to what we read, then copy the data we
            // read from the underlying transport. We do this because the
            // user data is passed to the reassembly queue without
            // copying.
            log::debug!("[{}] recving {} bytes", name, n);
            let inbound = Bytes::from(buffer[..n].to_vec());
            bytes_received.fetch_add(n, Ordering::SeqCst);

            {
                let mut ai = association_internal.lock().await;
                if let Err(err) = ai.handle_inbound(&inbound).await {
                    log::warn!("[{}] failed to handle_inbound: {:?}", name, err);
                    done = true;
                }
            }
        }

        {
            let mut ai = association_internal.lock().await;
            if let Err(err) = ai.close().await {
                log::warn!("[{}] failed to close association: {:?}", name, err);
            }
        }

        log::debug!("[{}] read_loop exited", name);
    }

    async fn write_loop(
        name: String,
        bytes_sent: Arc<AtomicUsize>,
        net_conn: Arc<dyn Conn + Send + Sync>,
        mut close_loop_ch: async_broadcast::Receiver<()>,
        association_internal: Arc<Mutex<AssociationInternal>>,
        awake_write_loop_ch: channel::Receiver<()>,
    ) {
        log::debug!("[{}] write_loop entered", name);
        let mut done = false;
        while !done {
            //log::debug!("[{}] gather_outbound begin", name);
            let (raw_packets, mut ok) = {
                let mut ai = association_internal.lock().await;
                ai.gather_outbound().await
            };
            //log::debug!("[{}] gather_outbound done with {}", name, raw_packets.len());

            for raw in &raw_packets {
                log::debug!("[{}] sending {} bytes", name, raw.len());
                if let Err(err) = net_conn.send(raw).await {
                    log::warn!("[{}] failed to write packets on net_conn: {}", name, err);
                    ok = false;
                    break;
                } else {
                    bytes_sent.fetch_add(raw.len(), Ordering::SeqCst);
                }
                //log::debug!("[{}] sending {} bytes done", name, raw.len());
            }

            if !ok {
                break;
            }

            //log::debug!("[{}] wait awake_write_loop_ch", name);
            select! {
                _ = awake_write_loop_ch.recv().fuse() =>{}
                _ = close_loop_ch.recv().fuse() => {
                    done = true;
                }
            };
            //log::debug!("[{}] wait awake_write_loop_ch done", name);
        }

        {
            let mut ai = association_internal.lock().await;
            if let Err(err) = ai.close().await {
                log::warn!("[{}] failed to close association: {:?}", name, err);
            }
        }

        log::debug!("[{}] write_loop exited", name);
    }

    /// open_stream opens a stream
    pub(crate) async fn open_stream(&self, stream_identifier: u16) -> Result<Arc<Stream>> {
        let mut ai = self.association_internal.lock().await;
        ai.open_stream(stream_identifier)
    }
}
