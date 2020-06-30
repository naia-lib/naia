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
    config.heartbeat_interval = Duration::from_secs(1);

    let mut server = NaiaServer::new(current_socket_address, manifest_load(), Some(config)).await;

    let main_room_key = server.create_room();
    let mut point_entities: Vec<Rc<RefCell<PointEntity>>> = Vec::new();
    for x in 0..20 {
        let point_entity = PointEntity::new(x, 0).wrap();
        point_entities.push(point_entity.clone());
        let entity_key = server.register_entity(point_entity);
        server.room_add_entity(&main_room_key, &entity_key);
    }

    server.on_scope_entity(Rc::new(Box::new(|_, _, _, entity| match entity {
        ExampleEntity::PointEntity(point_entity) => {
            let x = *point_entity.as_ref().borrow().x.get();
            return x >= 3 && x <= 17;
        }
    })));

    server.on_auth(Rc::new(Box::new(|_, auth_type| {
        if let ExampleEvent::AuthEvent(auth_event) = auth_type {
            let username = auth_event.username.get();
            let password = auth_event.password.get();
            return username == "charlie" && password == "12345";
        }
        return false;
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
                        // This could be used for your non-network logic (game loop?)

                        // Event Sending
                        let mut iter_vec: Vec<UserKey> = Vec::new();
                        for (user_key, _) in server.users_iter() {
                            iter_vec.push(user_key);
                        }
                        for user_key in iter_vec {
                            let user = server.get_user(&user_key).unwrap();
                            let new_message = "Server Packet (".to_string()
                                + tick_count.to_string().as_str()
                                + ") to "
                                + user.address.to_string().as_str();
                            info!("Naia Server send -> {}: {}", user.address, new_message);

                            let string_event = StringEvent::new(new_message);
                            server.send_event(&user_key, &string_event);
                        }

                        for point_entity in &point_entities {
                            point_entity.borrow_mut().step();
                        }

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
