use bevy::{
    ecs::{schedule::ShouldRun, world::World},
    log::LogPlugin,
    prelude::*,
};

use std::collections::HashMap;

use naia_server::{
    Event, Random, RoomKey, Server as NaiaServer, ServerAddrs, ServerConfig, UserKey,
};

use naia_bevy_demo_shared::{
    behavior as shared_behavior, get_server_address, get_shared_config,
    protocol::{Color, ColorValue, Position, Protocol},
};

use naia_bevy_server::{EntityKey, ToWorldMut};

type Server = NaiaServer<Protocol, EntityKey>;

static ALL: &str = "all";

struct ServerResource {
    pub server: Server,
    main_room_key: RoomKey,
    user_to_prediction_map: HashMap<UserKey, EntityKey>,
    ticked: bool,
}

fn main() {
    let mut app = App::build();

    // Plugins
    app.add_plugins(MinimalPlugins)
        .add_plugin(LogPlugin::default())
        .add_stage_before(CoreStage::PreUpdate, ALL, SystemStage::single_threaded());

    // Naia Server initialization
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

    let mut server = Server::new(ServerConfig::default(), get_shared_config());
    server.listen(server_addresses);

    // Create a new, singular room, which will contain Users and Entities that they
    // can receive updates from
    let main_room_key = server.make_room().key();

    // Resources
    app.insert_resource(ServerResource {
        server,
        main_room_key,
        user_to_prediction_map: HashMap::new(),
        ticked: false,
    });

    // Systems
    app.add_startup_system(init.system())
       .add_system_to_stage(ALL, naia_server_update.exclusive_system())
       .add_system_to_stage(ALL, on_tick.exclusive_system()
                                                         .with_run_criteria(
                                                             did_consume_tick.system()))

    // Run
       .run();
}

fn init() {
    info!("Naia Bevy Server Demo started");
}

fn naia_server_update(world: &mut World) {
    world.resource_scope(|world, mut resource: Mut<ServerResource>| {
        //let mut world_ref = WorldMut::new(world);
        let main_room_key = resource.main_room_key;

        for event in resource.server.receive(world.to_mut()) {
            match event {
                Ok(Event::Authorization(user_key, Protocol::Auth(auth_ref))) => {
                    let auth_message = auth_ref.borrow();
                    let username = auth_message.username.get();
                    let password = auth_message.password.get();
                    if username == "charlie" && password == "12345" {
                        // Accept incoming connection
                        resource.server.accept_connection(&user_key);
                    } else {
                        // Reject incoming connection
                        resource.server.reject_connection(&user_key);
                    }
                }
                Ok(Event::Connection(user_key)) => {
                    resource.server.room_mut(&main_room_key).add_user(&user_key);
                    let address = resource.server.user(&user_key).address();
                    info!("Naia Server connected to: {}", address);

                    // Create new Square Entity
                    let entity_key = resource.server.spawn_entity(world.to_mut()).key();

                    // Add Entity to main Room
                    resource
                        .server
                        .room_mut(&main_room_key)
                        .add_entity(&entity_key);

                    // Position component
                    {
                        // create
                        let mut x = Random::gen_range_u32(0, 40) as i16;
                        let mut y = Random::gen_range_u32(0, 30) as i16;
                        x -= 20;
                        y -= 15;
                        x *= 16;
                        y *= 16;
                        let position_ref = Position::new(x, y);

                        // add to entity
                        resource
                            .server
                            .entity_mut(world.to_mut(), &entity_key)
                            .insert_component(&position_ref);
                    }

                    // Color component
                    {
                        // create
                        let color_value = match resource.server.users_count() % 3 {
                            0 => ColorValue::Yellow,
                            1 => ColorValue::Red,
                            _ => ColorValue::Blue,
                        };
                        let color_ref = Color::new(color_value);

                        // add to entity
                        resource
                            .server
                            .entity_mut(world.to_mut(), &entity_key)
                            .insert_component(&color_ref);
                    }

                    // Assign as Prediction to User
                    resource
                        .server
                        .entity_mut(world.to_mut(), &entity_key)
                        .set_owner(&user_key);
                    resource.user_to_prediction_map.insert(user_key, entity_key);
                }
                Ok(Event::Disconnection(user_key, user)) => {
                    info!("Naia Server disconnected from: {:?}", user.address);

                    resource
                        .server
                        .room_mut(&main_room_key)
                        .remove_user(&user_key);
                    if let Some(naia_entity_key) = resource.user_to_prediction_map.remove(&user_key)
                    {
                        resource
                            .server
                            .room_mut(&main_room_key)
                            .remove_entity(&naia_entity_key);
                        resource
                            .server
                            .entity_mut(world.to_mut(), &naia_entity_key)
                            .despawn();
                    }
                }
                Ok(Event::Command(_, entity_key, Protocol::KeyCommand(key_command_ref))) => {
                    if let Some(position_ref) = resource
                        .server
                        .entity(world.to_mut(), &entity_key)
                        .component::<Position>()
                    {
                        shared_behavior::process_command(&key_command_ref, &position_ref);
                    }
                }
                Ok(Event::Tick) => {
                    resource.ticked = true;
                }
                Err(error) => {
                    info!("Naia Server error: {}", error);
                }
                _ => {}
            }
        }
    });
}

fn did_consume_tick(mut server_resource: ResMut<ServerResource>) -> ShouldRun {
    if server_resource.ticked {
        server_resource.ticked = false;
        return ShouldRun::Yes;
    }
    return ShouldRun::No;
}

fn on_tick(world: &mut World) {
    world.resource_scope(|world, mut resource: Mut<ServerResource>| {
        //let world_ref = WorldMut::new(world);

        // All game logic should happen here, on a tick event
        //info!("tick");

        // Update scopes of entities
        for (_, user_key, entity_key) in resource.server.scope_checks() {
            // You'd normally do whatever checks you need to in here..
            // to determine whether each Entity should be in scope or not.

            // This indicates the Entity should be in this scope.
            resource.server.user_scope(&user_key).include(&entity_key);

            // And call this if Entity should NOT be in this scope.
            // server.user_scope(..).exclude(..);
        }

        // VERY IMPORTANT! Calling this actually sends all update data
        // packets to all Clients that require it. If you don't call this
        // method, the Server will never communicate with it's connected Clients
        resource.server.send_all_updates(world.to_mut());
    });
}
