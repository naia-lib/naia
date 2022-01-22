use std::{thread, time::Duration};

use naia_server::{Event, Server as NaiaServer, ServerAddrs, ServerConfig};

use naia_tickless_demo_shared::{get_server_address, get_shared_config, Protocol, Text};

use naia_empty_world::{EmptyEntity, EmptyWorldRef};

type Server = NaiaServer<Protocol, EmptyEntity>;

pub struct App {
    server: Server,
}

impl App {
    pub fn new() -> Self {
        info!("Naia Tickless Server Demo started");

        let server_addresses = ServerAddrs::new(
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

        let mut server_config = ServerConfig::default();
        server_config.require_auth = false;
        server_config.disconnection_timeout_duration = Duration::from_secs(30);

        let mut server = Server::new(server_config, get_shared_config());
        server.listen(server_addresses);

        App { server }
    }

    pub fn update(&mut self) {
        for event in self.server.receive() {
            match event {
                Ok(Event::Connection(user_key)) => {
                    let user_address = self.server.user(&user_key).address();

                    info!("Naia Server connected to: {}", user_address);
                }
                Ok(Event::Disconnection(_, user)) => {
                    info!("Naia Server disconnected from: {}", user.address);
                }
                Ok(Event::Message(user_key, Protocol::Text(text))) => {
                    let client_message = text.value.get();
                    info!("Server recv <- {}", client_message);

                    let new_message_contents = format!("Server Message ({})", client_message);
                    info!("Server echo -> {}", new_message_contents);

                    let message = Text::new(&new_message_contents);
                    self.server.send_message(&user_key, &message, true);

                    // Sleep the thread to keep the demo from being unintelligibly fast
                    let sleep_time = Duration::from_millis(500);
                    thread::sleep(sleep_time);
                }
                Ok(Event::Tick) => {
                    info!("TICK SHOULD NOT HAPPEN!");
                }
                Err(error) => {
                    info!("Naia Server error: {}", error);
                }
                _ => {}
            }
        }

        self.server.send_all_updates(EmptyWorldRef::new());
    }
}
