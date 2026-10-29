use crate::webrtc::mux::mux_func::MatchFunc;
use crate::webrtc::util::{Buffer, Conn};

use async_trait::async_trait;
use std::io;
use std::net::SocketAddr;
use std::sync::Arc;

/// Endpoint implements net.Conn. It is used to read muxed packets.
pub(crate) struct Endpoint {
    pub(crate) id: usize,
    pub(crate) buffer: Buffer,
    pub(crate) match_fn: MatchFunc,
    pub(crate) next_conn: Arc<dyn Conn + Send + Sync>,
}

type Result<T> = std::result::Result<T, crate::webrtc::util::Error>;

#[async_trait]
impl Conn for Endpoint {
    /// reads a packet of len(p) bytes from the underlying conn
    /// that are matched by the associated MuxFunc
    async fn recv(&self, buf: &mut [u8]) -> Result<usize> {
        match self.buffer.read(buf, None).await {
            Ok(n) => Ok(n),
            Err(err) => Err(io::Error::other(err.to_string()).into()),
        }
    }
    async fn recv_from(&self, _buf: &mut [u8]) -> Result<(usize, SocketAddr)> {
        Err(io::Error::other("Not applicable").into())
    }

    /// writes bytes to the underlying conn
    async fn send(&self, buf: &[u8]) -> Result<usize> {
        self.next_conn.send(buf).await
    }

    async fn send_to(&self, _buf: &[u8], _target: SocketAddr) -> Result<usize> {
        Err(io::Error::other("Not applicable").into())
    }

    async fn local_addr(&self) -> Result<SocketAddr> {
        self.next_conn.local_addr().await
    }

    async fn remote_addr(&self) -> Option<SocketAddr> {
        self.next_conn.remote_addr().await
    }

    async fn close(&self) -> Result<()> {
        self.next_conn.close().await
    }
}
