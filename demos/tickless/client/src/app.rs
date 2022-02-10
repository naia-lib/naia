use log::info;

use naia_client::{Client as NaiaClient, ClientConfig, Event};

use naia_tickless_demo_shared::{get_server_address, get_shared_config, Protocol, Text};

use naia_empty_world::{EmptyEntity, EmptyWorldMut};

type Client = NaiaClient<Protocol, EmptyEntity>;

pub struct App {
    client: Client,
    message_count: u16,
}

impl App {
    pub fn new() -> Self {
        info!("Naia Tickless Client Demo started");

        let server_address = get_server_address();

        let mut client = Client::new(ClientConfig::default(), get_shared_config());
        client.connect(server_address);

        App {
            client,
            message_count: 0,
        }
    }

    pub fn update(&mut self) {
        for event in self.client.receive(EmptyWorldMut::new()) {
            match event {
                Ok(Event::Connection(server_address)) => {
                    info!("Client connected to: {}", server_address);

                    self.send_simple_message();
                }
                Ok(Event::Disconnection(server_address)) => {
                    info!("Client disconnected from: {}", server_address);
                }
                Ok(Event::Tick) => {
                    info!("TICK SHOULD NOT HAPPEN!");
                }
                Ok(Event::Message(Protocol::Text(text))) => {
                    info!("Client recv <- {}", text.value.get());

                    self.send_simple_message();
                }
                Err(err) => {
                    info!("Client Error: {}", err);
                }
                _ => {}
            }
        }
    }

    fn send_simple_message(&mut self) {
        let message_contents = format!("Client Message ({})", self.message_count);
        info!("Client send -> {}", message_contents);

        let message = Text::new(&message_contents);
        self.client.send_message(&message, true);
        self.message_count = self.message_count.wrapping_add(1);
    }
}
