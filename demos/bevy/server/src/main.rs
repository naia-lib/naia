use bevy::{
    ecs::{entity::Entity as BevyEntityKey, schedule::ShouldRun},
    log::LogPlugin,
    prelude::*,
};

use std::collections::HashMap;

use naia_server::{
    EntityKey as NaiaEntityKey, Event, Random, Ref, RoomKey, Server, ServerConfig, UserKey,
};

use naia_bevy_demo_shared::{
    behavior as shared_behavior, get_server_address, get_shared_config,
    protocol::{Color, ColorValue, Position, Protocol},
};

static ALL: &str = "all";

// Resource definitions
struct ServerResource {
    main_room_key: RoomKey,
    naia_to_bevy_key_map: HashMap<NaiaEntityKey, BevyEntityKey>,
    bevy_to_naia_key_map: HashMap<BevyEntityKey, NaiaEntityKey>,
    user_to_prediction_map: HashMap<UserKey, NaiaEntityKey>,
    ticked: bool,
}

fn main() {
    let mut app = App::build();

    // Plugins
    app.add_plugins(MinimalPlugins)
        .add_plugin(LogPlugin::default())
        .add_stage_before(CoreStage::PreUpdate, ALL, SystemStage::single_threaded());

    // Naia Server initialization
    let mut server_config = ServerConfig::default();
    server_config.socket_config.session_listen_addr = get_server_address();
    let mut server = Server::new(Some(server_config), get_shared_config());

    // Create a new, singular room, which will contain Users and Entities that they
    // can receive updates from
    let main_room_key = server.make_room().key();

    // Resources
    app.insert_non_send_resource(server);
    app.insert_resource(ServerResource {
        main_room_key,
        naia_to_bevy_key_map: HashMap::new(),
        bevy_to_naia_key_map: HashMap::new(),
        user_to_prediction_map: HashMap::new(),
        ticked: false,
    });

    // Systems
    app.add_startup_system(init.system())
       .add_system_to_stage(ALL, naia_server_update.system())
       .add_system_to_stage(ALL, on_tick.system()
                                                         .with_run_criteria(
                                                             did_consume_tick.system()))

    // Run
       .run();
}

fn init() {
    info!("Naia Bevy Server Demo started");
}

fn naia_server_update(
    mut commands: Commands,
    mut server: NonSendMut<Server<Protocol>>,
    mut server_resource: ResMut<ServerResource>,
    mut c_q: Query<&Ref<Position>>,
) {
    for event in server.receive() {
        match event {
            Ok(Event::Authorization(user_key, Protocol::Auth(auth_ref))) => {
                let auth_message = auth_ref.borrow();
                let username = auth_message.username.get();
                let password = auth_message.password.get();
                if username == "charlie" && password == "12345" {
                    // Accept incoming connection
                    server.accept_connection(&user_key);
                } else {
                    // Reject incoming connection
                    server.reject_connection(&user_key);
                }
            }
            Ok(Event::Connection(user_key)) => {
                server
                    .room_mut(&server_resource.main_room_key)
                    .add_user(&user_key);
                let address = server.user(&user_key).address();
                info!("Naia Server connected to: {}", address);

                // Create new Square Entity in Naia
                let naia_entity_key = server.spawn_entity().key();

                // Create new Square Entity in Bevy
                let mut bevy_entity = commands.spawn();
                let bevy_entity_key = bevy_entity.id();

                // Update sync map
                server_resource
                    .naia_to_bevy_key_map
                    .insert(naia_entity_key, bevy_entity_key);
                server_resource
                    .bevy_to_naia_key_map
                    .insert(bevy_entity_key, naia_entity_key);

                // Add Naia Entity to main Room
                server
                    .room_mut(&server_resource.main_room_key)
                    .add_entity(&naia_entity_key);

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

                    // add to Naia
                    server
                        .entity_mut(&naia_entity_key)
                        .insert_component(&position_ref);

                    // add to Bevy
                    bevy_entity.insert(Ref::clone(&position_ref));
                }

                // Color component
                {
                    // create
                    let color_value = match server.users_count() % 3 {
                        0 => ColorValue::Yellow,
                        1 => ColorValue::Red,
                        _ => ColorValue::Blue,
                    };
                    let color_ref = Color::new(color_value);

                    // add to Naia
                    server
                        .entity_mut(&naia_entity_key)
                        .insert_component(&color_ref);

                    // add to Bevy
                    bevy_entity.insert(Ref::clone(&color_ref));
                }

                // Assign as Prediction to User
                server.user_mut(&user_key).own_entity(&naia_entity_key);
                server_resource
                    .user_to_prediction_map
                    .insert(user_key, naia_entity_key);
            }
            Ok(Event::Disconnection(user_key, user)) => {
                info!("Naia Server disconnected from: {:?}", user.address);

                server
                    .room_mut(&server_resource.main_room_key)
                    .remove_user(&user_key);
                if let Some(naia_entity_key) =
                    server_resource.user_to_prediction_map.remove(&user_key)
                {
                    server
                        .room_mut(&server_resource.main_room_key)
                        .remove_entity(&naia_entity_key);
                    server.user_mut(&user_key).disown_entity(&naia_entity_key);
                    server.entity_mut(&naia_entity_key).despawn();
                    if let Some(bevy_entity_key) = server_resource
                        .naia_to_bevy_key_map
                        .remove(&naia_entity_key)
                    {
                        commands.entity(bevy_entity_key).despawn();
                        server_resource
                            .bevy_to_naia_key_map
                            .remove(&bevy_entity_key);
                    }
                }
            }
            Ok(Event::Command(_, naia_entity, Protocol::KeyCommand(key_command_ref))) => {
                if let Some(bevy_entity) = server_resource.naia_to_bevy_key_map.get(&naia_entity) {
                    if let Ok(position_ref) = c_q.get_mut(*bevy_entity) {
                        shared_behavior::process_command(&key_command_ref, position_ref);
                    }
                }
            }
            Ok(Event::Tick) => {
                server_resource.ticked = true;
            }
            Err(error) => {
                info!("Naia Server error: {}", error);
            }
            _ => {}
        }
    }
}

fn did_consume_tick(mut server_resource: ResMut<ServerResource>) -> ShouldRun {
    if server_resource.ticked {
        server_resource.ticked = false;
        return ShouldRun::Yes;
    }
    return ShouldRun::No;
}

fn on_tick(mut server: NonSendMut<Server<Protocol>>) {
    // All game logic should happen here, on a tick event
    //info!("tick");

    // Update scopes of entities
    for (room_key, user_key, naia_entity_key) in server.scopes() {
        // You'd normally do whatever checks you need to in here..
        // to determine whether each Entity should be in scope or not.

        // This indicates the Entity should be in this scope.
        server.accept_scope(room_key, user_key, naia_entity_key);

        // And call this if Entity should NOT be in this scope.
        // server.reject_scope(...);
    }

    // VERY IMPORTANT! Calling this actually sends all update data
    // packets to all Clients that require it. If you don't call this
    // method, the Server will never communicate with it's connected Clients
    server.send_all_updates();
}
