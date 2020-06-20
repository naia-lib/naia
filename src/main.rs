
#[macro_use]
extern crate log;

use simple_logger;

use gaia_server::{GaiaServer, ServerEvent, Entity, find_my_ip_address, Config};

use gaia_example_shared::{manifest_load, PointEntity, ExampleEvent, ExampleEntity};

use std::{
    rc::Rc,
    cell::RefCell,
    time::Duration};

const SERVER_PORT: &str = "3179";

//TODO: GET RID OF THIS...
pub fn get_server_point_entity(e: &Rc<RefCell<PointEntity>>) -> Rc<RefCell<dyn Entity<ExampleEntity>>> {
    e.clone() as Rc<RefCell<dyn Entity<ExampleEntity>>>
}

#[tokio::main]
async fn main() {

    simple_logger::init_with_level(log::Level::Info).expect("A logger was already initialized");

    let current_socket_address = find_my_ip_address::get() + ":" + SERVER_PORT;

    let mut config = Config::default();
    config.tick_interval = Duration::from_secs(4);
    config.heartbeat_interval = Duration::from_secs(1);

    let mut server = GaiaServer::listen(current_socket_address.as_str(),
                                        manifest_load(),
                                        Some(config)).await;

    let main_room_key = server.create_room();
    let mut point_entities: Vec<Rc<RefCell<PointEntity>>> = Vec::new();
    for x in 0..20 {
        let point_entity = PointEntity::new(x, 0);
        let server_point_entity: Rc<RefCell<dyn Entity<ExampleEntity>>> = get_server_point_entity(&point_entity);
        let entity_key = server.register_entity(&server_point_entity);
        let main_room = server.get_room_mut(main_room_key).unwrap();
        main_room.add_entity(&entity_key);
        point_entities.push(point_entity.clone());
    }

    server.on_scope_entity(Rc::new(Box::new(|_, _, _, entity| {
        match entity {
            ExampleEntity::PointEntity(point_entity) => {
                let x = point_entity.as_ref().borrow().get_x();
                return x >= 3 && x <= 17;
            }
        }
    })));

    server.on_auth(Rc::new(Box::new(|user_key, auth_type| {
        if let ExampleEvent::AuthEvent(auth_event) = auth_type {
            let username = auth_event.get_username();
            let password = auth_event.get_password();
            return username == "charlie" && password == "12345";
        }
        return false;
    })));

    loop {
        match server.receive().await {
            Ok(event) => {
                match event {
                    ServerEvent::Connection(user_key) => {
                        if let Some(main_room) = server.get_room_mut(main_room_key) {
                            main_room.subscribe_user(&user_key);
                        }
                        if let Some(user) = server.get_user(&user_key) {
                            info!("Gaia Server connected to: {}", user.address);
                        }
                    }
                    ServerEvent::Disconnection(_, user) => {
                        info!("Gaia Server disconnected from: {:?}", user.address);
                    }
                    ServerEvent::Event(user_key, event_type) => {
                        if let Some(user) = server.get_user(&user_key) {
                            match event_type {
                                ExampleEvent::StringEvent(string_event) => {
                                    let message = string_event.get_message();
                                    match message {
                                        Some(msg) => {
                                            info!("Gaia Server recv <- {}: {}", user.address, msg);
                                        }
                                        None => {}
                                    }
                                }
                                _ => {}
                            }
                        }
                    }
                    ServerEvent::Tick => {
                        // This could be used for your non-network logic (game loop?)

                        // Event Sending
//                        for addr in server.get_clients() {
//                            let count = server.get_sequence_number(addr).expect("why don't we have a sequence number for this client?");
//                            let new_message = "Server Packet (".to_string() + count.to_string().as_str() + ") to " + addr.to_string().as_str();
//                            info!("Gaia Server send -> {}: {}", addr, new_message);
//
//                            let string_event = StringEvent::new(new_message);
//                            server.send_event(addr, &string_event);
//                        }

                        for point_entity in &point_entities {
                            point_entity.borrow_mut().step();
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