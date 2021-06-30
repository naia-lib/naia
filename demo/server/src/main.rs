#[macro_use]
extern crate log;

use simple_logger;
use smol::io;

use naia_server::{NaiaServer, ServerAddresses, ServerConfig, ServerEvent, UserKey};

use naia_example_shared::{
    get_shared_config, manifest_load, ExampleActor, ExampleEvent, PointActor, StringEvent,
};

use std::{
    rc::Rc,
    time::Duration,
};

fn main() -> io::Result<()> {
    let server_addresses: ServerAddresses = ServerAddresses::new(
        // IP Address to listen on for the signaling portion of WebRTC
        "127.0.0.1:14191"
            .parse()
            .expect("could not parse HTTP address/port"),
        // IP Address to listen on for UDP WebRTC data channels
        "127.0.0.1:14192"
            .parse()
            .expect("could not parse WebRTC data address/port"),
        // The public WebRTC IP address to advertise
        "127.0.0.1:14192"
            .parse()
            .expect("could not parse advertised public WebRTC data address/port"),
    );

    smol::block_on(async {
        simple_logger::init_with_level(log::Level::Info).expect("A logger was already initialized");

        info!("Naia Server Example Started");

        let mut server_config = ServerConfig::default();
        server_config.heartbeat_interval = Duration::from_secs(2);
        // Keep in mind that the disconnect timeout duration should always be at least
        // 2x greater than the heartbeat interval, to make it so at the worst case, the
        // server would need to miss 2 heartbeat signals before disconnecting from a
        // given client
        server_config.disconnection_timeout_duration = Duration::from_secs(5);

        let mut server = NaiaServer::new(
            server_addresses,
            manifest_load(),
            Some(server_config),
            get_shared_config(),
        )
        .await;

        // This method is called during the connection handshake process, and can be
        // used to reject a new connection if the correct credentials have not been
        // provided
        server.on_auth(Rc::new(Box::new(|_, auth_type| {
            if let ExampleEvent::AuthEvent(auth_event) = auth_type {
                let username = auth_event.username.get();
                let password = auth_event.password.get();
                return username == "charlie" && password == "12345";
            }
            return false;
        })));

        // Create a new, singular room, which will contain Users and Actors that they
        // can receive updates from
        let main_room_key = server.create_room();

        // Create 4 PointActors, with a range of X values
        {
            let mut count = 0;
            for (first, last) in [
                ("alpha", "red"),
                ("bravo", "blue"),
                ("charlie", "green"),
                ("delta", "yellow"),
            ]
            .iter()
            {
                count += 1;
                let point_actor = PointActor::new((count * 4) as u8, 0, first, last).wrap();
                let actor_key = server.register_actor(ExampleActor::PointActor(point_actor));
                server.room_add_actor(&main_room_key, &actor_key);
            }
        }

        // This method will be called every step to determine whether a given Actor
        // should be in scope for a given User
        server.on_scope_actor(Rc::new(Box::new(|_, _, _, actor| match actor {
            ExampleActor::PointActor(point_actor) => {
                let x = *point_actor.borrow().x.get();
                // Currently, a PointActor is only in scope if it's X value is between 5 & 15.
                // This could be configured to some value within a User's current viewport, for
                // example
                return x >= 5 && x <= 15;
            }
        })));

        let mut tick_count: u32 = 0;

        loop {
            match server.receive().await {
                Ok(event) => {
                    match event {
                        ServerEvent::Connection(user_key) => {
                            server.room_add_user(&main_room_key, &user_key);
                            if let Some(user) = server.get_user(&user_key) {
                                info!("Naia Server connected to: {}", user.address);
                            }
                        }
                        ServerEvent::Disconnection(_, user) => {
                            info!("Naia Server disconnected from: {:?}", user.address);
                        }
                        ServerEvent::Event(user_key, event_type) => {
                            if let Some(user) = server.get_user(&user_key) {
                                match event_type {
                                    ExampleEvent::StringEvent(string_event) => {
                                        let message = string_event.message.get();
                                        info!("Naia Server recv <- {}: {}", user.address, message);
                                    }
                                    _ => {}
                                }
                            }
                        }
                        ServerEvent::Tick => {
                            // Game logic, updating of the world, should happen here

                            // Event Sending
                            let mut iter_vec: Vec<UserKey> = Vec::new();
                            for (user_key, _) in server.users_iter() {
                                iter_vec.push(user_key);
                            }
                            for user_key in iter_vec {
                                let user = server.get_user(&user_key).unwrap();
                                let new_message = format!("Server Packet ({})", tick_count);
                                info!("Naia Server send -> {}: {}", user.address, new_message);

                                let string_event = StringEvent::new(new_message);
                                server.queue_event(&user_key, &string_event);
                            }

                            // Iterate through Point Actors, marching them from (0,0) to (20, N)
                            for (_, actor) in server.actors_iter() {
                                match actor {
                                    ExampleActor::PointActor(point_actor) => {
                                        point_actor.borrow_mut().step();
                                    }
                                }
                            }

                            // VERY IMPORTANT! Calling this actually sends all Actor/Event data
                            // packets to all Clients that require it. If you don't call this
                            // method, the Server will never communicate with it's connected Clients
                            server.send_all_updates().await;

                            tick_count = tick_count.wrapping_add(1);
                        }
                        _ => {}
                    }
                }
                Err(error) => {
                    info!("Naia Server Error: {}", error);
                }
            }
        }
    })
}
