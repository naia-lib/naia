#[macro_use]
extern crate log;

use std::time::Duration;

use log::LevelFilter;
use simple_logger::SimpleLogger;
use smol::io;

use naia_server::{ServerAddresses, ServerConfig};

use naia_basic_demo_shared::get_server_address;

mod app;
use app::App;

fn main() -> io::Result<()> {
    SimpleLogger::new()
        .with_level(LevelFilter::Info)
        .init()
        .expect("A logger was already initialized");

    info!("Basic Naia Server Demo Started");

    let mut server_config = ServerConfig::default();

    server_config.socket_addresses = ServerAddresses::new(
        // IP Address to listen on for the signaling portion of WebRTC
        get_server_address(),
        // IP Address to listen on for UDP WebRTC data channels
        "127.0.0.1:14192"
            .parse()
            .expect("could not parse WebRTC data address/port"),
        // The public WebRTC IP address to advertise
        "127.0.0.1:14192"
            .parse()
            .expect("could not parse advertised public WebRTC data address/port"),
    );

    server_config.heartbeat_interval = Duration::from_secs(2);
    // Keep in mind that the disconnect timeout duration should always be at least
    // 2x greater than the heartbeat interval, to make it so at the worst case, the
    // server would need to miss 2 heartbeat signals before disconnecting from a
    // given client
    server_config.disconnection_timeout_duration = Duration::from_secs(5);

    smol::block_on(async {
        let mut app = App::new(server_config).await;
        loop {
            app.update().await;
        }
    })
}
