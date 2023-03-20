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
    packet_sender: Box<dyn PacketSender>,
    packet_receiver: Box<dyn PacketReceiver>,
    message_count: u8,
    timer: Timer,
    server_addr_str: Option<String>,
}

impl App {
    pub fn new() -> App {
        info!("Naia Client Socket Demo started");

        let (packet_sender, packet_receiver) =
            Socket::connect("http://127.0.0.1:14191", &shared_config());

        App {
            packet_sender,
            packet_receiver,
            message_count: 0,
            timer: Timer::new(Duration::from_secs(1)),
            server_addr_str: None,
        }
    }

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

                    if self.message_count == 10 {
                        info!("Client finished sending messages");
                    }
                }
            }
            Ok(None) => {
                if self.message_count < 10 {
                    if self.timer.ringing() {
                        self.timer.reset();

                        let message_to_server: String = PING_MSG.to_string();

                        let server_addr = match self.packet_receiver.server_addr() {
                            ServerAddr::Found(addr) => addr.to_string(),
                            _ => "".to_string(),
                        };
                        info!("Client send -> {}: {}", server_addr, message_to_server);

                        match self.packet_sender.send(message_to_server.as_bytes()) {
                            Ok(()) => {}
                            Err(error) => {
                                info!("Client Send Error: {}", error);
                            }
                        }
                    }
                }
            }
            Err(err) => {
                info!("Client Error: {}", err);
            }
        }
    }
}
