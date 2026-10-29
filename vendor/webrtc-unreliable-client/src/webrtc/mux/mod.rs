pub(crate) mod endpoint;
pub(crate) mod mux_func;

use crate::webrtc::error::Result;
use crate::webrtc::mux::endpoint::Endpoint;
use crate::webrtc::mux::mux_func::MatchFunc;

use crate::webrtc::util::{Buffer, Conn};
use crate::webrtc::RECEIVE_MTU;
use async_lock::Mutex;
use futures_util::{select, FutureExt};
use smol::channel;
use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

// mux multiplexes packets on a single socket (RFC7983)

/// The maximum amount of data that can be buffered before returning errors.
const MAX_BUFFER_SIZE: usize = 1000 * 1000; // 1MB

/// Config collects the arguments to mux.Mux construction into
/// a single structure
pub(crate) struct Config {
    pub(crate) conn: Arc<dyn Conn + Send + Sync>,
}

/// Mux allows multiplexing
#[derive(Clone)]
pub(crate) struct Mux {
    id: Arc<AtomicUsize>,
    next_conn: Arc<dyn Conn + Send + Sync>,
    endpoints: Arc<Mutex<HashMap<usize, Arc<Endpoint>>>>,
    closed_ch_tx: Option<channel::Sender<()>>,
}

impl Mux {
    pub(crate) fn new(config: Config) -> Self {
        let (closed_ch_tx, closed_ch_rx) = channel::bounded(1);
        let m = Mux {
            id: Arc::new(AtomicUsize::new(0)),
            next_conn: Arc::clone(&config.conn),
            endpoints: Arc::new(Mutex::new(HashMap::new())),
            closed_ch_tx: Some(closed_ch_tx),
        };

        let next_conn = Arc::clone(&m.next_conn);
        let endpoints = Arc::clone(&m.endpoints);
        smol::spawn(async move {
            Mux::read_loop(next_conn, closed_ch_rx, endpoints).await;
        })
        .detach();

        m
    }

    /// creates a new Endpoint
    pub(crate) async fn new_endpoint(&self, f: MatchFunc) -> Arc<Endpoint> {
        let mut endpoints = self.endpoints.lock().await;

        let id = self.id.fetch_add(1, Ordering::SeqCst);
        // Set a maximum size of the buffer in bytes.
        // NOTE: We actually won't get anywhere close to this limit.
        // SRTP will constantly read from the endpoint and drop packets if it's full.
        let e = Arc::new(Endpoint {
            id,
            buffer: Buffer::new(0, MAX_BUFFER_SIZE),
            match_fn: f,
            next_conn: Arc::clone(&self.next_conn),
        });

        endpoints.insert(e.id, Arc::clone(&e));

        e
    }

    async fn read_loop(
        next_conn: Arc<dyn Conn + Send + Sync>,
        closed_ch_rx: channel::Receiver<()>,
        endpoints: Arc<Mutex<HashMap<usize, Arc<Endpoint>>>>,
    ) {
        let mut buf = vec![0u8; RECEIVE_MTU];
        let mut n = 0usize;
        loop {
            select! {
                _ = closed_ch_rx.recv().fuse() => break,
                result = next_conn.recv(&mut buf).fuse() => {
                    if let Ok(m) = result{
                        n = m;
                    }
                }
            };

            if let Err(err) = Mux::dispatch(&buf[..n], &endpoints).await {
                log::error!("mux: ending readLoop dispatch error {:?}", err);
                break;
            }
        }
    }

    async fn dispatch(
        buf: &[u8],
        endpoints: &Arc<Mutex<HashMap<usize, Arc<Endpoint>>>>,
    ) -> Result<()> {
        let mut endpoint = None;

        {
            let eps = endpoints.lock().await;
            for ep in eps.values() {
                if (ep.match_fn)(buf) {
                    endpoint = Some(Arc::clone(ep));
                    break;
                }
            }
        }

        if let Some(ep) = endpoint {
            ep.buffer.write(buf).await?;
        } else if !buf.is_empty() {
            log::warn!(
                "Warning: mux: no endpoint for packet starting with {}",
                buf[0]
            );
        } else {
            log::warn!("Warning: mux: no endpoint for zero length packet");
        }

        Ok(())
    }

    pub(crate) async fn close(&mut self) {
        if let Some(closed_ch_tx) = self.closed_ch_tx.take() {
            let _ = closed_ch_tx.send(()).await;
        }
    }
}
