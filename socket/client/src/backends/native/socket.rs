extern crate log;

use crossbeam::channel;

use webrtc_unreliable_client::{ServerAddr, Socket as RTCSocket};
use naia_socket_shared::{parse_server_url, SocketConfig};

use tokio::runtime::{Builder, Runtime};

use crate::{
    conditioned_packet_receiver::ConditionedPacketReceiver,
    io::Io,
    packet_receiver::{PacketReceiver, PacketReceiverTrait},
};

use super::{packet_receiver::PacketReceiverImpl, packet_sender::PacketSender};

/// A client-side socket which communicates with an underlying unordered &
/// unreliable protocol
pub struct Socket {
    config: SocketConfig,
    io: Option<Io>,
}

impl Socket {
    /// Create a new Socket
    pub fn new(config: &SocketConfig) -> Self {
        Socket {
            config: config.clone(),
            io: None,
        }
    }

    /// Connects to the given server address
    pub fn connect(&mut self, server_session_url: &str) {
        if self.io.is_some() {
            panic!("Socket already listening!");
        }

        let server_url = parse_server_url(server_session_url);
        let server_session_string = format!("{}{}", server_url, self.config.rtc_endpoint_path.clone()).to_string();

        // Setup sync channels
        let (from_server_sender, from_server_receiver) = channel::unbounded();
        let (sender_sender, sender_receiver) = channel::bounded(1);
        let (addr_sender, addr_receiver) = channel::bounded(1);

        let conditioner_config = self.config.link_condition.clone();

        {
            let detached = runtime.spawn_blocking(async move {
                let (addr_cell, to_server_sender, mut to_client_receiver) = RTCSocket::connect(&server_session_string).await;

                sender_sender.send(to_server_sender).unwrap();
                //TODO: handle result

                let mut found_addr: Option<SocketAddr> = None;

                loop {
                    if let Some(message) = to_client_receiver.recv().await {
                        from_server_sender.send(message).unwrap();
                        //TODO: handle result

                        if found_addr.is_none() {
                            if let ServerAddr::Found(addr) = addr_cell.get().await {
                                addr_sender.send(addr).unwrap();
                                //TODO: handle result
                            }
                        }
                    }
                }
            });
        }

        // Set up sender loop
        let (to_server_sender, to_server_receiver) = channel::unbounded();

        {
            let detached = runtime.spawn_blocking(async move {
                loop {
                    // Create async socket
                    if let Ok(mut async_sender) = sender_receiver.recv() {
                        loop {
                            if let Ok(msg) = to_server_receiver.recv() {
                                async_sender.send(msg).await.unwrap();
                                //TODO: handle result..
                            }
                        }
                    }
                }
            });
        }

        // Setup Packet Sender & Receiver
        let addr_cell = AddrCell::new(addr_receiver);
        let packet_sender = PacketSender::new(addr_cell.clone(), to_server_sender);
        let packet_receiver_impl = PacketReceiverImpl::new(addr_cell, from_server_receiver);

        let receiver: Box<dyn PacketReceiverTrait> = {
            let inner_receiver = Box::new(packet_receiver_impl);
            if let Some(config) = &conditioner_config {
                Box::new(ConditionedPacketReceiver::new(inner_receiver, config))
            } else {
                inner_receiver
            }
        };


        self.io = Some(Io {
            packet_sender,
            packet_receiver: PacketReceiver::new(receiver),
        });
    }

    /// Gets a PacketSender which can be used to send packets through the Socket
    pub fn packet_sender(&self) -> PacketSender {
        return self
            .io
            .as_ref()
            .expect("Socket is not connected yet! Call Socket.connect() before this.")
            .packet_sender
            .clone();
    }

    /// Gets a PacketReceiver which can be used to receive packets from the
    /// Socket
    pub fn packet_receiver(&self) -> PacketReceiver {
        return self
            .io
            .as_ref()
            .expect("Socket is not connected yet! Call Socket.connect() before this.")
            .packet_receiver
            .clone();
    }
}

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use log::warn;
use crate::backends::native::addr_cell::AddrCell;

/// Helper method to find local IP address, if possible
pub fn find_my_ip_address() -> Option<IpAddr> {
    let ip = local_ipaddress::get().unwrap_or_default();

    if let Ok(addr) = ip.parse::<Ipv4Addr>() {
        Some(IpAddr::V4(addr))
    } else if let Ok(addr) = ip.parse::<Ipv6Addr>() {
        Some(IpAddr::V6(addr))
    } else {
        None
    }
}
