use std::{
    io::Error as IoError,
    net::{SocketAddr, UdpSocket},
};

use log::info;

use async_io::Async;
use futures_channel::mpsc;
use futures_util::{pin_mut, select, FutureExt, StreamExt};

use naia_socket_shared::SocketConfig;

use crate::{error::NaiaServerSocketError, server_addrs::ServerAddrs};

const CLIENT_CHANNEL_SIZE: usize = 8;

/// A socket server which communicates with clients using an underlying
/// unordered & unreliable network protocol
pub struct Socket {
    socket: Async<UdpSocket>,
    to_client_sender: mpsc::Sender<(SocketAddr, Box<[u8]>)>,
    to_client_receiver: mpsc::Receiver<(SocketAddr, Box<[u8]>)>,
    receive_buffer: Vec<u8>,
}

impl Socket {
    /// Returns a new ServerSocket, listening at the given socket address
    pub async fn listen(addrs: ServerAddrs, _config: SocketConfig) -> Self {
        let socket = Async::new(UdpSocket::bind(&addrs.session_listen_addr).unwrap()).unwrap();

        let (to_client_sender, to_client_receiver) = mpsc::channel(CLIENT_CHANNEL_SIZE);

        info!(
            "UDP server listening on socket: {}",
            addrs.session_listen_addr
        );

        Socket {
            socket,
            to_client_sender,
            to_client_receiver,
            receive_buffer: vec![0; 0x10000], /* Hopefully get rid of this one day.. next version
                                               * of webrtc-unreliable should make that happen */
        }
    }

    pub async fn receive(&mut self) -> Result<(SocketAddr, Box<[u8]>), NaiaServerSocketError> {
        enum Next {
            FromClientMessage(Result<(usize, SocketAddr), IoError>),
            ToClientMessage((SocketAddr, Box<[u8]>)),
        }

        loop {
            let next = {
                let to_client_receiver_next = self.to_client_receiver.next().fuse();
                pin_mut!(to_client_receiver_next);

                let receive_buffer = &mut self.receive_buffer;
                let udp_socket = &mut self.socket;
                let from_client_message_receiver_next = udp_socket.recv_from(receive_buffer).fuse();
                pin_mut!(from_client_message_receiver_next);

                select! {
                    from_client_result = from_client_message_receiver_next => {
                        Next::FromClientMessage(from_client_result)
                    }
                    to_client_message = to_client_receiver_next => {
                        Next::ToClientMessage(
                            to_client_message.expect("to server message receiver closed")
                        )
                    }
                }
            };

            match next {
                Next::FromClientMessage(from_client_message) => match from_client_message {
                    Ok((message_len, message_address)) => {
                        return Ok((message_address, self.receive_buffer[0..message_len].into()));
                    }
                    Err(err) => {
                        return Err(NaiaServerSocketError::Wrapped(Box::new(err)));
                    }
                },
                Next::ToClientMessage((address, payload)) => {
                    if (self.socket.send_to(&payload, address).await).is_err() {
                        return Err(NaiaServerSocketError::SendError(address));
                    }
                }
            }
        }
    }

    pub fn sender(&self) -> mpsc::Sender<(SocketAddr, Box<[u8]>)> {
        self.to_client_sender.clone()
    }
}
