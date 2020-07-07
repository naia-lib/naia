#[macro_use]
extern crate log;

use simple_logger;

use naia_server::{find_my_ip_address, Config, NaiaServer, ServerEvent, UserKey};

use naia_example_shared::{manifest_load, ExampleEntity, ExampleEvent, PointEntity, StringEvent};

use std::{cell::RefCell, net::SocketAddr, rc::Rc, time::Duration};

const SERVER_PORT: u16 = 14191;

#[tokio::main]
async fn main() {
    simple_logger::init_with_level(log::Level::Info).expect("A logger was already initialized");

    info!("Naia Server Example Started");

    let current_ip_address = find_my_ip_address().expect("can't find ip address");
    let current_socket_address = SocketAddr::new(current_ip_address, SERVER_PORT);

    let mut config = Config::default();
    config.tick_interval = Duration::from_secs(4);
    config.heartbeat_interval = Duration::from_secs(2);
    // Keep in mind that the disconnect timeout duration should always be at least
    // 2x greater than the heartbeat interval, to make it so at the worst case, the
    // server would need to miss 2 heartbeat signals before disconnecting from a
    // given client
    config.disconnection_timeout_duration = Duration::from_secs(5);

    let mut server = NaiaServer::new(current_socket_address, manifest_load(), Some(config)).await;

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

    // Create a new, singular room, which will contain Users and Entities that they
    // can receive updates from
    let main_room_key = server.create_room();

    // Create 10 PointEntities, with a range of X values
    let mut point_entities: Vec<Rc<RefCell<PointEntity>>> = Vec::new();
    for x in 0..10 {
        let point_entity = PointEntity::new(x * 2, 0).wrap();
        point_entities.push(point_entity.clone());
        let entity_key = server.register_entity(point_entity);
        server.room_add_entity(&main_room_key, &entity_key);
    }

    // This method will be called every step to determine whether a given Entity
    // should be in scope for a given User
    server.on_scope_entity(Rc::new(Box::new(|_, _, _, entity| match entity {
        ExampleEntity::PointEntity(point_entity) => {
            let x = *point_entity.as_ref().borrow().x.get();
            // Currently, a PointEntity is only in scope if it's X value is between 5 & 15.
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

                        // Iterate through Point Entities, marching them from (0,0) to (20, N)
                        for point_entity in &point_entities {
                            point_entity.borrow_mut().step();
                        }

                        // VERY IMPORTANT! Calling this actually sends all Entity/Event data packets
                        // to all Clients that require it. If you don't call this method, the Server
                        // will never communicate with it's connected Clients
                        server.send_all_updates().await;

                        tick_count += 1;
                    }
                }
            }
            Err(error) => {
                info!("Naia Server Error: {}", error);
            }
        }
    }
}
