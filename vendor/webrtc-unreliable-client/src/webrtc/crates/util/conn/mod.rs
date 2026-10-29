pub(crate) mod conn_udp;

use std::net::SocketAddr;

use async_trait::async_trait;

use crate::webrtc::util::error::Result;

#[async_trait]
pub(crate) trait Conn {
    async fn recv(&self, buf: &mut [u8]) -> Result<usize>;
    async fn recv_from(&self, buf: &mut [u8]) -> Result<(usize, SocketAddr)>;
    async fn send(&self, buf: &[u8]) -> Result<usize>;
    async fn send_to(&self, buf: &[u8], target: SocketAddr) -> Result<usize>;
    async fn local_addr(&self) -> Result<SocketAddr>;
    async fn remote_addr(&self) -> Option<SocketAddr>;
    async fn close(&self) -> Result<()>;
}
