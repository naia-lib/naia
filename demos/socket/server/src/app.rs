use std::{thread::sleep, time::Duration};

use naia_server_socket::{
    AuthReceiver, AuthSender, PacketReceiver, PacketSender, ServerAddrs, Socket,
};

use naia_socket_demo_shared::{shared_config, PING_MSG, PONG_MSG};

pub struct App {
    auth_sender: Box<dyn AuthSender>,
    auth_receiver: Box<dyn AuthReceiver>,
    packet_sender: Box<dyn PacketSender>,
    packet_receiver: Box<dyn PacketReceiver>,
}

impl App {
    pub fn new() -> Self {
        info!("Naia Server Socket Demo started");

        let server_address = ServerAddrs::new(
            "127.0.0.1:14191"
                .parse()
                .expect("could not parse Session address/port"),
            // IP Address to listen on for UDP WebRTC data channels
            "127.0.0.1:14192"
                .parse()
                .expect("could not parse WebRTC data address/port"),
            // The public WebRTC IP address to advertise
            "http://127.0.0.1:14192",
        );
        let shared_config = shared_config();

        let (auth_sender, auth_receiver, packet_sender, packet_receiver) =
            Socket::listen_with_auth(&server_address, &shared_config);

        Self {
            auth_sender,
            auth_receiver,
            packet_sender,
            packet_receiver,
        }
    }

    pub fn update(&mut self) {
        let mut no_auths = false;
        let mut no_packets = false;

        match self.auth_receiver.receive() {
            Ok(Some((address, payload))) => {
                let auth_from_client = String::from_utf8_lossy(payload);
                info!(
                    "Server incoming Auth <- {}: [{}]",
                    address, auth_from_client
                );

                if auth_from_client.eq("12345") {
                    if let Err(error) = self.auth_sender.accept(&address) {
                        info!("Server Accept Auth error {}", error);
                    } else {
                        info!("Server Auth accepted: {}", address);
                    }
                } else {
                    if let Err(error) = self.auth_sender.reject(&address) {
                        info!("Server Reject Auth error {}", error);
                    } else {
                        info!("Server Auth rejected: {}", address);
                    }
                }
            }
            Ok(None) => {
                no_auths = true;
            }
            Err(error) => {
                info!("Server Auth Error: {}", error);
            }
        }
        match self.packet_receiver.receive() {
            Ok(Some((address, payload))) => {
                let message_from_client = String::from_utf8_lossy(payload);
                info!("Server recv <- {}: {}", address, message_from_client);

                if message_from_client.eq(PING_MSG) {
                    let message_to_client: String = PONG_MSG.to_string();
                    info!("Server send -> {}: {}", address, message_to_client);
                    match self
                        .packet_sender
                        .send(&address, message_to_client.as_bytes())
                    {
                        Ok(()) => {}
                        Err(error) => {
                            info!("Server Send Error {}", error);
                        }
                    }
                }
            }
            Ok(None) => {
                no_packets = true;
            }
            Err(error) => {
                info!("Server Error: {}", error);
            }
        }

        if no_auths && no_packets {
            // If we don't sleep here, app will loop at 100% CPU until a new message comes in
            sleep(Duration::from_millis(1));
        }
    }
}
