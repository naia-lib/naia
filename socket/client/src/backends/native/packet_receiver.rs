use std::{
    io::ErrorKind,
    net::{SocketAddr, UdpSocket},
    sync::{Arc, Mutex},
};

use crate::{
    error::NaiaClientSocketError, packet_receiver::PacketReceiverTrait, server_addr::ServerAddr,
};

/// Handles receiving messages from the Server through a given Client Socket
#[derive(Clone)]
pub struct PacketReceiverImpl {
    server_addr: SocketAddr,
    local_socket: Arc<Mutex<UdpSocket>>,
    receive_buffer: Vec<u8>,
}

impl PacketReceiverImpl {
    /// Create a new PacketReceiver, if supplied with the Server's address & a
    /// reference back to the parent Socket
    pub fn new(server_addr: SocketAddr, local_socket: Arc<Mutex<UdpSocket>>) -> Self {
        PacketReceiverImpl {
            server_addr,
            local_socket,
            receive_buffer: vec![0; 1472],
        }
    }
}

impl PacketReceiverTrait for PacketReceiverImpl {
    fn receive(&mut self) -> Result<Option<&[u8]>, NaiaClientSocketError> {
        let buffer: &mut [u8] = self.receive_buffer.as_mut();
        match self.local_socket.as_ref().lock().unwrap().recv_from(buffer) {
            Ok((recv_len, address)) => {
                if address == self.server_addr {
                    Ok(Some(&buffer[..recv_len]))
                } else {
                    let err_message = format!(
                        "Received packet from unknown sender with a socket address of: {}",
                        address
                    );
                    Err(NaiaClientSocketError::Message(err_message))
                }
            }
            Err(ref e) if e.kind() == ErrorKind::WouldBlock => {
                //just didn't receive anything this time
                Ok(None)
            }
            Err(e) => Err(NaiaClientSocketError::Wrapped(Box::new(e))),
        }
    }

    /// Get the Server's Socket address
    fn server_addr(&self) -> ServerAddr {
        ServerAddr::Found(self.server_addr)
    }
}
