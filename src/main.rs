
#[macro_use]
extern crate log;

use simple_logger;

use gaia_server::{GaiaServer, ServerEvent, find_my_ip_address};

const SERVER_PORT: &str = "3179";

#[tokio::main]
async fn main() {

    simple_logger::init_with_level(log::Level::Info).expect("A logger was already initialized");

    let current_socket_address = find_my_ip_address::get() + ":" + SERVER_PORT;

    let mut server = GaiaServer::listen(current_socket_address.as_str()).await;

    let mut count = 0;

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
                    }
                    ServerEvent::Tick => {
                        // This could be used for your non-network logic (game loop?)
                        count += 1;
                        if count > 299 {
                            count = 0;
                        }
                        for addr in server.get_clients() {
                            let new_message = "Server Packet ".to_string() + count.to_string().as_str();
                            info!("Gaia Server send -> {}: {}", addr, new_message);
                            server.send((addr, new_message))
                                .await;
                        }
                    }
                }
            }
            Err(error) => {
                info!("Gaia Server Error: {}", error);
            }
        }
    }
}