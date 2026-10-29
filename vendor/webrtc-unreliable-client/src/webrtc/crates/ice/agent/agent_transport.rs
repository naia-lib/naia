use super::*;
use crate::webrtc::ice::error::*;

use crate::webrtc::util::Conn;
use async_trait::async_trait;
use std::io;
use std::sync::atomic::{AtomicBool, Ordering};

impl Agent {
    /// Connects to the remote agent, acting as the controlling ice agent.
    /// The method blocks until at least one ice candidate pair has successfully connected.
    ///
    /// The operation will be cancelled if `cancel_rx` either receives a message or its channel
    /// closes.
    pub(crate) async fn dial(
        &self,
        cancel_rx: channel::Receiver<()>,
        remote_ufrag: String,
        remote_pwd: String,
    ) -> Result<Arc<impl Conn>> {
        let (on_connected_rx, agent_conn) = {
            self.internal
                .start_connectivity_checks(true, remote_ufrag, remote_pwd)
                .await?;

            let mut on_connected_rx = self.internal.on_connected_rx.lock().await;
            (
                on_connected_rx.take(),
                Arc::clone(&self.internal.agent_conn),
            )
        };

        if let Some(on_connected_rx) = on_connected_rx {
            // block until pair selected
            select! {
                _ = on_connected_rx.recv().fuse() => {},
                _ = cancel_rx.recv().fuse() => {
                    return Err(Error::ErrCanceledByCaller);
                }
            }
        }
        Ok(agent_conn)
    }

    /// Connects to the remote agent, acting as the controlled ice agent.
    /// The method blocks until at least one ice candidate pair has successfully connected.
    ///
    /// The operation will be cancelled if `cancel_rx` either receives a message or its channel
    /// closes.
    pub(crate) async fn accept(
        &self,
        cancel_rx: channel::Receiver<()>,
        remote_ufrag: String,
        remote_pwd: String,
    ) -> Result<Arc<impl Conn>> {
        let (on_connected_rx, agent_conn) = {
            self.internal
                .start_connectivity_checks(false, remote_ufrag, remote_pwd)
                .await?;

            let mut on_connected_rx = self.internal.on_connected_rx.lock().await;
            (
                on_connected_rx.take(),
                Arc::clone(&self.internal.agent_conn),
            )
        };

        if let Some(on_connected_rx) = on_connected_rx {
            // block until pair selected
            select! {
                _ = on_connected_rx.recv().fuse() => {},
                _ = cancel_rx.recv().fuse() => {
                    return Err(Error::ErrCanceledByCaller);
                }
            }
        }

        Ok(agent_conn)
    }
}

pub(crate) struct AgentConn {
    pub(crate) selected_pair: Mutex<Option<Arc<CandidatePair>>>,
    pub(crate) checklist: Mutex<Vec<Arc<CandidatePair>>>,

    pub(crate) buffer: Buffer,
    pub(crate) bytes_received: AtomicUsize,
    pub(crate) bytes_sent: AtomicUsize,
    pub(crate) done: AtomicBool,
}

impl AgentConn {
    pub(crate) fn new() -> Self {
        Self {
            selected_pair: Mutex::new(None),
            checklist: Mutex::new(vec![]),
            // Make sure the buffer doesn't grow indefinitely.
            // NOTE: We actually won't get anywhere close to this limit.
            // SRTP will constantly read from the endpoint and drop packets if it's full.
            buffer: Buffer::new(0, MAX_BUFFER_SIZE),
            bytes_received: AtomicUsize::new(0),
            bytes_sent: AtomicUsize::new(0),
            done: AtomicBool::new(false),
        }
    }
    pub(crate) async fn get_selected_pair(&self) -> Option<Arc<CandidatePair>> {
        let selected_pair = self.selected_pair.lock().await;
        selected_pair.clone()
    }

    pub(crate) async fn get_best_available_candidate_pair(&self) -> Option<Arc<CandidatePair>> {
        let mut best: Option<&Arc<CandidatePair>> = None;

        let checklist = self.checklist.lock().await;
        for p in &*checklist {
            if p.state.load(Ordering::SeqCst) == CandidatePairState::Failed as u8 {
                continue;
            }

            if let Some(b) = &mut best {
                if b.priority() < p.priority() {
                    *b = p;
                }
            } else {
                best = Some(p);
            }
        }

        best.cloned()
    }

    pub(crate) async fn get_best_valid_candidate_pair(&self) -> Option<Arc<CandidatePair>> {
        let mut best: Option<&Arc<CandidatePair>> = None;

        let checklist = self.checklist.lock().await;
        for p in &*checklist {
            if p.state.load(Ordering::SeqCst) != CandidatePairState::Succeeded as u8 {
                continue;
            }

            if let Some(b) = &mut best {
                if b.priority() < p.priority() {
                    *b = p;
                }
            } else {
                best = Some(p);
            }
        }

        best.cloned()
    }
}

#[async_trait]
impl Conn for AgentConn {
    async fn recv(&self, buf: &mut [u8]) -> std::result::Result<usize, crate::webrtc::util::Error> {
        if self.done.load(Ordering::SeqCst) {
            return Err(io::Error::other("Conn is closed").into());
        }

        let n = match self.buffer.read(buf, None).await {
            Ok(n) => n,
            Err(err) => return Err(io::Error::other(err.to_string()).into()),
        };
        self.bytes_received.fetch_add(n, Ordering::SeqCst);

        Ok(n)
    }

    async fn recv_from(
        &self,
        buf: &mut [u8],
    ) -> std::result::Result<(usize, SocketAddr), crate::webrtc::util::Error> {
        if let Some(raddr) = self.remote_addr().await {
            let n = self.recv(buf).await?;
            Ok((n, raddr))
        } else {
            Err(io::Error::other("Not applicable").into())
        }
    }

    async fn send(&self, buf: &[u8]) -> std::result::Result<usize, crate::webrtc::util::Error> {
        if self.done.load(Ordering::SeqCst) {
            return Err(io::Error::other("Conn is closed").into());
        }

        if is_message(buf) {
            return Err(crate::webrtc::util::Error::Other(
                "ErrIceWriteStunMessage".into(),
            ));
        }

        let result = if let Some(pair) = self.get_selected_pair().await {
            pair.write(buf).await
        } else if let Some(pair) = self.get_best_available_candidate_pair().await {
            pair.write(buf).await
        } else {
            Ok(0)
        };

        match result {
            Ok(n) => {
                self.bytes_sent.fetch_add(buf.len(), Ordering::SeqCst);
                Ok(n)
            }
            Err(err) => Err(io::Error::other(err.to_string()).into()),
        }
    }

    async fn send_to(
        &self,
        _buf: &[u8],
        _target: SocketAddr,
    ) -> std::result::Result<usize, crate::webrtc::util::Error> {
        Err(io::Error::other("Not applicable").into())
    }

    async fn local_addr(&self) -> std::result::Result<SocketAddr, crate::webrtc::util::Error> {
        if let Some(pair) = self.get_selected_pair().await {
            Ok(pair.local.addr().await)
        } else {
            Err(io::Error::new(io::ErrorKind::AddrNotAvailable, "Addr Not Available").into())
        }
    }

    async fn remote_addr(&self) -> Option<SocketAddr> {
        if let Some(pair) = self.get_selected_pair().await {
            Some(pair.remote.addr().await)
        } else {
            None
        }
    }

    async fn close(&self) -> std::result::Result<(), crate::webrtc::util::Error> {
        Ok(())
    }
}
