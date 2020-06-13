
#[macro_use]
extern crate log;

use simple_logger;

use gaia_server::{GaiaServer, ServerEvent, NetEntity, find_my_ip_address, Config};

use gaia_example_shared::{event_manifest_load, entity_manifest_load, PointEntity, ExampleEvent, ExampleEntity};

use std::{
    rc::Rc,
    cell::RefCell,
    time::Duration};

const SERVER_PORT: &str = "3179";

pub fn get_server_point_entity(e: &Rc<RefCell<PointEntity>>) -> Rc<RefCell<dyn NetEntity<ExampleEntity>>> {
    e.clone() as Rc<RefCell<dyn NetEntity<ExampleEntity>>>
}

#[tokio::main]
async fn main() {

    simple_logger::init_with_level(log::Level::Info).expect("A logger was already initialized");

    let current_socket_address = find_my_ip_address::get() + ":" + SERVER_PORT;

    let mut config = Config::default();
    config.tick_interval = Duration::from_secs(4);
    config.heartbeat_interval = Duration::from_secs(1);

    let mut server = GaiaServer::listen(current_socket_address.as_str(),
                                        event_manifest_load(),
                                        entity_manifest_load(),
                                        Some(config)).await;

    let mut point_entities: Vec<Rc<RefCell<PointEntity>>> = Vec::new();
    for x in 0..20 {
        let point_entity = PointEntity::new(x, 0);
        let server_point_entity: Rc<RefCell<dyn NetEntity<ExampleEntity>>> = get_server_point_entity(&point_entity);
        server.add_entity(&server_point_entity);
        point_entities.push(point_entity.clone());
    }

    server.on_scope_entity(Rc::new(Box::new(|address, entity| {
        match entity {
            ExampleEntity::PointEntity(point_entity) => {
                let x = point_entity.as_ref().borrow().get_x();
                return x >= 3 && x <= 17;
            }
        }
    })));

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
                    ServerEvent::Event(address, event_type) => {
                        match event_type {
                            ExampleEvent::StringEvent(string_event) => {
                                let message = string_event.get_message();
                                match message {
                                    Some(msg) => {
                                        info!("Gaia Server recv <- {}: {}", address, msg);
                                    }
                                    None => {}
                                }
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