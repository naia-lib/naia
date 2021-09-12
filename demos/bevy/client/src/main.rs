use std::collections::HashMap;

use bevy::{ecs::entity::Entity as BevyEntityKey, prelude::*};

use naia_client::{Client, ClientConfig, Event, LocalEntityKey as NaiaEntityKey, Ref};

use naia_bevy_demo_shared::{
    behavior as shared_behavior, get_server_address, get_shared_config,
    protocol::{Auth, ColorValue, KeyCommand, Position, Protocol},
};

const SQUARE_SIZE: f32 = 32.0;
static ALL: &str = "all";

struct Pawn;
struct NonPawn;
struct Key(NaiaEntityKey);
struct Materials {
    white: Handle<ColorMaterial>,
    red: Handle<ColorMaterial>,
    blue: Handle<ColorMaterial>,
    yellow: Handle<ColorMaterial>,
}
struct QueuedCommand {
    command: Option<Ref<KeyCommand>>,
}
struct ClientResource {
    entity_key_map: HashMap<NaiaEntityKey, BevyEntityKey>,
    pawn_key_map: HashMap<NaiaEntityKey, BevyEntityKey>,
}

fn main() {
    let mut app = App::build();

    // Plugins
    app.add_plugins(DefaultPlugins).add_stage_before(
        CoreStage::PreUpdate,
        ALL,
        SystemStage::single_threaded(),
    );

    #[cfg(target_arch = "wasm32")]
    app.add_plugin(bevy_webgl2::WebGL2Plugin);

    // This will be evaluated in the Server's 'on_auth()' method
    let auth = Auth::new("charlie", "12345");

    // Add Naia Client
    let mut client_config = ClientConfig::default();
    client_config.socket_config.server_address = get_server_address();
    let client = Client::new(
        Protocol::load(),
        Some(client_config),
        get_shared_config(),
        Some(auth),
    );
    app.insert_non_send_resource(client);

    // Resources
    app.insert_non_send_resource(QueuedCommand { command: None })
        .insert_resource(ClearColor(Color::rgb(0.0, 0.0, 0.0)))
        .insert_resource(ClientResource {
            entity_key_map: HashMap::new(),
            pawn_key_map: HashMap::new(),
        });

    // Systems
    app.add_startup_system(init.system())
       .add_system(pawn_input.system())
       .add_system_to_stage(ALL, naia_client_update.system())
       .add_system_to_stage(ALL, pawn_sync.system())
       .add_system_to_stage(ALL, nonpawn_sync.system())

    // Run
       .run();
}

fn init(mut commands: Commands, mut materials: ResMut<Assets<ColorMaterial>>) {
    info!("Naia Bevy Client Demo started");

    // Setup Camera
    commands.spawn_bundle(OrthographicCameraBundle::new_2d());

    // Setup Colors
    commands.insert_resource(Materials {
        white: materials.add(Color::rgb(1.0, 1.0, 1.0).into()),
        red: materials.add(Color::rgb(1.0, 0.0, 0.0).into()),
        blue: materials.add(Color::rgb(0.0, 0.0, 1.0).into()),
        yellow: materials.add(Color::rgb(1.0, 1.0, 0.0).into()),
    });
}

fn pawn_input(keyboard_input: Res<Input<KeyCode>>, mut queued_command: NonSendMut<QueuedCommand>) {
    let w = keyboard_input.pressed(KeyCode::W);
    let s = keyboard_input.pressed(KeyCode::S);
    let a = keyboard_input.pressed(KeyCode::A);
    let d = keyboard_input.pressed(KeyCode::D);

    if let Some(command_ref) = &mut queued_command.command {
        let mut command = command_ref.borrow_mut();
        if w {
            command.w.set(true);
        }
        if s {
            command.s.set(true);
        }
        if a {
            command.a.set(true);
        }
        if d {
            command.d.set(true);
        }
    } else {
        queued_command.command = Some(KeyCommand::new(w, s, a, d));
    }
}

fn naia_client_update(
    mut commands: Commands,
    mut client: NonSendMut<Client<Protocol>>,
    mut client_resource: ResMut<ClientResource>,
    materials: Res<Materials>,
    mut pawn_query: Query<(&Key, &Ref<Position>), With<Pawn>>,
    mut queued_command: NonSendMut<QueuedCommand>,
) {
    for event in client.receive() {
        match event {
            Ok(Event::Connection) => {
                info!("Client connected to: {}", client.server_address());
            }
            Ok(Event::Disconnection) => {
                info!("Client disconnected from: {}", client.server_address());
            }
            Ok(Event::Tick) => {
                for (Key(pawn_key), _) in pawn_query.iter() {
                    if let Some(command) = queued_command.command.take() {
                        client.send_entity_command(pawn_key, &command);
                    }
                }
            }
            Ok(Event::SpawnEntity(naia_entity_key, component_list)) => {
                let mut entity = commands.spawn();
                entity.insert(NonPawn).insert(Key(naia_entity_key));

                info!("create entity");

                for component_protocol in component_list {
                    match component_protocol {
                        Protocol::Position(position_ref) => {
                            info!("add position to entity");
                            entity.insert(position_ref);
                        }
                        Protocol::Color(color_ref) => {
                            info!("add color to entity");
                            entity.insert(Ref::clone(&color_ref));
                            let color = color_ref.borrow();

                            let material = {
                                match &color.value.get() {
                                    ColorValue::Red => materials.red.clone(),
                                    ColorValue::Blue => materials.blue.clone(),
                                    ColorValue::Yellow => materials.yellow.clone(),
                                }
                            };

                            entity.insert_bundle(SpriteBundle {
                                material: material.clone(),
                                sprite: Sprite::new(Vec2::new(SQUARE_SIZE, SQUARE_SIZE)),
                                transform: Transform::from_xyz(0.0, 0.0, 0.0),
                                ..Default::default()
                            });
                        }
                        _ => {}
                    }
                }

                let bevy_entity_key = entity.id();
                client_resource
                    .entity_key_map
                    .insert(naia_entity_key, bevy_entity_key);
            }
            Ok(Event::DespawnEntity(naia_entity_key)) => {
                if let Some(bevy_entity_key) =
                    client_resource.entity_key_map.remove(&naia_entity_key)
                {
                    commands.entity(bevy_entity_key).despawn();
                }
            }
            Ok(Event::AssignPawnEntity(naia_entity_key)) => {
                info!("assign pawn");

                let pawn_components = client.get_pawn_components(&naia_entity_key);
                if !pawn_components.is_empty() {
                    let mut entity = commands.spawn();
                    entity.insert(Pawn).insert(Key(naia_entity_key));

                    for pawn_component in pawn_components {
                        match pawn_component {
                            Protocol::Position(position_ref) => {
                                info!("add position to pawn entity");
                                entity.insert(position_ref);
                            }
                            _ => {}
                        }
                    }

                    entity.insert_bundle(SpriteBundle {
                        material: materials.white.clone(),
                        sprite: Sprite::new(Vec2::new(SQUARE_SIZE, SQUARE_SIZE)),
                        transform: Transform::from_xyz(0.0, 0.0, 0.0),
                        ..Default::default()
                    });

                    let bevy_entity_key = entity.id();
                    client_resource
                        .pawn_key_map
                        .insert(naia_entity_key, bevy_entity_key);
                }
            }
            Ok(Event::UnassignPawn(naia_entity_key)) => {
                info!("unassign pawn");

                if let Some(bevy_entity_key) = client_resource.pawn_key_map.remove(&naia_entity_key)
                {
                    commands.entity(bevy_entity_key).despawn();
                }
            }
            Ok(Event::NewCommandEntity(naia_entity, Protocol::KeyCommand(key_command_ref)))
            | Ok(Event::ReplayCommandEntity(naia_entity, Protocol::KeyCommand(key_command_ref))) => {
                if let Some(bevy_entity) = client_resource.pawn_key_map.get(&naia_entity) {
                    if let Ok((_, position)) = pawn_query.get_mut(*bevy_entity) {
                        shared_behavior::process_command(&key_command_ref, position);
                    }
                }
            }
            _ => {}
        }
    }
}

fn pawn_sync(mut query: Query<(&Pawn, &Ref<Position>, &mut Transform)>) {
    for (_, pos_ref, mut transform) in query.iter_mut() {
        let pos = pos_ref.borrow();
        transform.translation.x = f32::from(*(pos.x.get()));
        transform.translation.y = f32::from(*(pos.y.get())) * -1.0;
    }
}

fn nonpawn_sync(mut query: Query<(&NonPawn, &Ref<Position>, &mut Transform)>) {
    for (_, pos_ref, mut transform) in query.iter_mut() {
        let pos = pos_ref.borrow();
        transform.translation.x = f32::from(*(pos.x.get()));
        transform.translation.y = f32::from(*(pos.y.get())) * -1.0;
    }
}
