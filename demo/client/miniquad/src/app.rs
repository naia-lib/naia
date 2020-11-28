use miniquad::info;

use std::{net::SocketAddr, time::Duration};

use naia_client::{ClientConfig, ClientEvent, NaiaClient};

use naia_example_shared::{
    get_shared_config, manifest_load, AuthEvent, ExampleActor, ExampleEvent, StringEvent,
};

const SERVER_PORT: u16 = 14191;

cfg_if! {
    if #[cfg(target_arch = "wasm32")] {
        use std::net::IpAddr;
    } else {
        use naia_client::find_my_ip_address;
    }
}

pub struct App {
    client: NaiaClient<ExampleEvent, ExampleActor>,
    server_event_count: u32,
}

impl App {
    pub fn new() -> App {
        info!("Naia Client Example Started");

        cfg_if! {
            if #[cfg(target_arch = "wasm32")] {
                // Put your Server's IP Address here!, can't easily find this automatically from the browser
                let server_ip_address: IpAddr = "192.168.86.38".parse().expect("couldn't parse input IP address");
            } else {
                let server_ip_address = find_my_ip_address().expect("can't find ip address");
            }
        }

        let server_socket_address = SocketAddr::new(server_ip_address, SERVER_PORT);

        let mut client_config = ClientConfig::default();
        client_config.heartbeat_interval = Duration::from_secs(2);
        // Keep in mind that the disconnect timeout duration should always be at least
        // 2x greater than the heartbeat interval, to make it so at the worst case, the
        // server would need to miss 2 heartbeat signals before disconnecting from a
        // given client
        client_config.disconnection_timeout_duration = Duration::from_secs(5);

        // This will be evaluated in the Server's 'on_auth()' method
        let auth = ExampleEvent::AuthEvent(AuthEvent::new("charlie", "12345"));

        App {
            client: NaiaClient::new(
                server_socket_address,
                manifest_load(),
                Some(client_config),
                get_shared_config(),
                Some(auth),
            ),
            server_event_count: 0,
        }
    }

    // Currently, this will call every frame. On Linux it's called in a loop. On Web
    // it's called via request_animation_frame()
    pub fn update(&mut self) {
        loop {
            if let Some(result) = self.client.receive() {
                match result {
                    Ok(event) => match event {
                        ClientEvent::Connection => {
                            info!("Client connected to: {}", self.client.server_address());
                        }
                        ClientEvent::Disconnection => {
                            info!("Client disconnected from: {}", self.client.server_address());
                        }
                        ClientEvent::Event(event_type) => match event_type {
                            ExampleEvent::StringEvent(string_event) => {
                                let message = string_event.message.get();
                                info!("Client received event: {}", message);

                                let new_message =
                                    format!("Client Packet ({})", self.server_event_count);
                                info!("Client send: {}", new_message);

                                let string_event = StringEvent::new(new_message);
                                self.client.send_event(&string_event);
                                self.server_event_count += 1;
                            }
                            _ => {}
                        },
                        ClientEvent::CreateActor(local_key) => {
                            if let Some(actor) = self.client.get_actor(&local_key) {
                                match actor {
                                    ExampleActor::PointActor(point_actor) => {
                                        info!("creation of point actor with key: {}, x: {}, y: {}, name: {} {}",
                                              local_key,
                                              point_actor.as_ref().borrow().x.get(),
                                              point_actor.as_ref().borrow().y.get(),
                                              point_actor.as_ref().borrow().name.get().first,
                                              point_actor.as_ref().borrow().name.get().last,
                                        );
                                    }
                                }
                            }
                        }
                        ClientEvent::UpdateActor(local_key) => {
                            if let Some(actor) = self.client.get_actor(&local_key) {
                                match actor {
                                    ExampleActor::PointActor(point_actor) => {
                                        info!("update of point actor with key: {}, x:{}, y: {}, name: {} {}",
                                              local_key,
                                              point_actor.as_ref().borrow().x.get(),
                                              point_actor.as_ref().borrow().y.get(),
                                              point_actor.as_ref().borrow().name.get().first,
                                              point_actor.as_ref().borrow().name.get().last);
                                    }
                                }
                            }
                        }
                        ClientEvent::DeleteActor(local_key) => {
                            info!("deletion of point actor with key: {}", local_key);
                        }
                        ClientEvent::Tick => {
                            //info!("tick event");
                        }
                        _ => {}
                    },
                    Err(err) => {
                        info!("Client Error: {}", err);
                        return;
                    }
                }
            } else {
                break;
            }
        }
    }
}
