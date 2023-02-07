use std::error::Error;
use std::net::SocketAddr;

pub trait SocketConfig: {}

pub trait Socket {
    fn new(config: impl SocketConfig) -> Self;
    fn recv(&mut self, buffer: &mut [u8]) -> Result<Option<(usize, SocketAddr)>, Box<dyn Error + Send + Sync + 'static>>;
    fn send_to(&mut self, buffer: &[u8], addr: SocketAddr) -> Result<(), Box<dyn Error + Send + Sync + 'static>>;
}