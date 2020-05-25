
#[macro_use]
extern crate log;

use simple_logger;

use gaia_server::{GaiaServer, ServerEvent, find_my_ip_address};

const SERVER_PORT: &str = "3179";
const PING_MSG: &str = "ping";
const PONG_MSG: &str = "pong";

#[tokio::main]
async fn main() {

    simple_logger::init_with_level(log::Level::Info).expect("A logger was already initialized");

    let current_socket_address = find_my_ip_address::get() + ":" + SERVER_PORT;

    let mut server = GaiaServer::listen(current_socket_address.as_str()).await;

    loop {
        match server.receive().await {
            Ok(event) => {
                match event {
                    ServerEvent::Connection(address) => {
                        info!("Gaia Server connected to: {}", address);
                    }
                    ServerEvent::Disconnection(address) => {
                        info!("Gaia Server disconnected from: {:?}", address);
                    }
                    ServerEvent::Message(address, message) => {
                        info!("Gaia Server recv <- {}: {}", address, message);

                        if message.eq(PING_MSG) {
                            let to_client_message: String = PONG_MSG.to_string();
                            info!("Gaia Server send -> {}: {}", address, to_client_message);
                            server.send((address, to_client_message))
                                .await.expect("send error");
                        }
                    }
                    ServerEvent::Tick => {
                        // This could be used for your non-network logic (game loop?)
                    }
                }
            }
            Err(error) => {
                info!("Gaia Server Error: {}", error);
            }
        }
    }
}