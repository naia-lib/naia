//! smol-based implementation of webrtc-unreliable 0.6's `Runtime` trait, so the
//! RTC server can run on naia's existing smol executor instead of tokio.

use std::{
    future::Future,
    io,
    net::SocketAddr,
    pin::Pin,
    task::{Context, Poll},
    time::Duration,
};

use futures_util::ready;
use webrtc_unreliable::runtime;

pub struct Timer(smol::Timer);

impl Future for Timer {
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context) -> Poll<()> {
        match Pin::new(&mut self.0).poll(cx) {
            Poll::Ready(_instant) => Poll::Ready(()),
            Poll::Pending => Poll::Pending,
        }
    }
}

pub struct UdpSocket(smol::Async<std::net::UdpSocket>);

impl runtime::UdpSocket for UdpSocket {
    fn poll_recv_from(
        &mut self,
        cx: &mut Context,
        buf: &mut [u8],
    ) -> Poll<Result<(usize, SocketAddr), io::Error>> {
        loop {
            match self.0.get_ref().recv_from(buf) {
                Ok(res) => return Poll::Ready(Ok(res)),
                Err(err) if err.kind() == io::ErrorKind::WouldBlock => {
                    ready!(self.0.poll_readable(cx))?;
                }
                Err(err) => return Poll::Ready(Err(err)),
            }
        }
    }

    fn poll_send_to(
        &mut self,
        cx: &mut Context,
        buf: &[u8],
        addr: SocketAddr,
    ) -> Poll<Result<usize, io::Error>> {
        loop {
            match self.0.get_ref().send_to(buf, addr) {
                Ok(len) => return Poll::Ready(Ok(len)),
                Err(err) if err.kind() == io::ErrorKind::WouldBlock => {
                    ready!(self.0.poll_writable(cx))?;
                }
                Err(err) => return Poll::Ready(Err(err)),
            }
        }
    }
}

pub struct SmolRuntime;

impl runtime::Runtime for SmolRuntime {
    type Timer = Timer;
    type UdpSocket = UdpSocket;

    fn bind_udp(&self, listen_addr: SocketAddr) -> Result<UdpSocket, io::Error> {
        Ok(UdpSocket(smol::Async::new(std::net::UdpSocket::bind(
            listen_addr,
        )?)?))
    }

    fn timer(&self, after: Duration) -> Timer {
        Timer(smol::Timer::after(after))
    }
}
