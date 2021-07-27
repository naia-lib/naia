use std::time::Duration;

use naia_client::ClientConfig;

pub struct Config;

impl Config {
    pub fn get() -> ClientConfig {
        let mut client_config = ClientConfig::default();

        // Put your Server's IP Address here!, can't easily find this automatically from
        // the browser
        client_config.server_address = "127.0.0.1:14191"
            .parse()
            .expect("couldn't parse input IP address");

        client_config.heartbeat_interval = Duration::from_secs(2);
        // Keep in mind that the disconnect timeout duration should always be at least
        // 2x greater than the heartbeat interval, to make it so at the worst case, the
        // server would need to miss 2 heartbeat signals before disconnecting from a
        // given client
        client_config.disconnection_timeout_duration = Duration::from_secs(5);

        client_config
    }
}