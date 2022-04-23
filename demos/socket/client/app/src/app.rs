use std::time::Duration;

cfg_if! {
    if #[cfg(feature = "mquad")] {
        use miniquad::info;
    } else {
        use log::info;
    }
}

use naia_client_socket::{PacketReceiver, PacketSender, ServerAddr, Socket};

use naia_shared::Timer;

use naia_socket_demo_shared::{shared_config, PING_MSG, PONG_MSG};

pub struct App {
    packet_sender: PacketSender,
    packet_receiver: PacketReceiver,
    message_count: u8,
    timer: Timer,
    server_addr_str: Option<String>,
}

impl Default for App {
    fn default() -> App {
        info!("Naia Client Socket Demo started");

        let mut socket = Socket::new(&shared_config());
        socket.connect("http://127.0.0.1:14191");

        App {
            packet_sender: socket.packet_sender(),
            packet_receiver: socket.packet_receiver(),
            message_count: 0,
            timer: Timer::new(Duration::from_secs(1)),
            server_addr_str: None,
        }
    }
}

impl App {
    pub fn update(&mut self) {
        if self.server_addr_str.is_none() {
            if let ServerAddr::Found(addr) = self.packet_receiver.server_addr() {
                self.server_addr_str = Some(addr.to_string());
            }
        }

        match self.packet_receiver.receive() {
            Ok(Some(packet)) => {
                let message_from_server = String::from_utf8_lossy(packet);

                info!(
                    "Client recv <- {}: {}",
                    self.server_addr_str.as_ref().unwrap_or(&"".to_string()),
                    message_from_server
                );

                if message_from_server.eq(PONG_MSG) {
                    self.message_count += 1;
                }
            }
            Ok(None) => {
                if self.timer.ringing() {
                    self.timer.reset();
                    if self.message_count < 10 {
                        let message_to_server: String = PING_MSG.to_string();

                        let server_addr = match self.packet_receiver.server_addr() {
                            ServerAddr::Found(addr) => addr.to_string(),
                            _ => "".to_string(),
                        };
                        info!("Client send -> {}: {}", server_addr, message_to_server);

                        self.packet_sender.send(message_to_server.as_bytes());
                    }
                }
            }
            Err(err) => {
                info!("Client Error: {}", err);
            }
        }
    }
}
