use bevy::prelude::*;

use naia_client::{
    Client, ClientConfig, Event, LocalReplicaKey, Ref
};

use naia_bevy_demo_shared::{
    behavior as shared_behavior, get_server_address, get_shared_config,
    protocol::{Auth, Color as SquareColor, KeyCommand, Protocol, Square},
};

const SQUARE_SIZE: f32 = 32.0;
static ALL: &str = "all";

struct Pawn;
struct NonPawn;
struct Key(LocalReplicaKey);
struct Materials {
    white: Handle<ColorMaterial>,
    red: Handle<ColorMaterial>,
    blue: Handle<ColorMaterial>,
    yellow: Handle<ColorMaterial>,
}
struct QueuedCommand {
    command: Option<Ref<KeyCommand>>,
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

    // Add Naia Client
    let mut client_config = ClientConfig::default();
    client_config.socket_config.server_address = get_server_address();

    // This will be evaluated in the Server's 'on_auth()' method
    let auth = Auth::new("charlie", "12345");

    // insert
    app.insert_non_send_resource(Client::new(
        Protocol::load(),
        Some(client_config),
        get_shared_config(),
        Some(auth),
    ));

    // Resources
    app.insert_non_send_resource(QueuedCommand { command: None })
       .insert_resource(ClearColor(Color::rgb(0.0, 0.0, 0.0)));

    // Systems
    app.add_startup_system(setup.system())
       .add_system(pawn_input.system())
       .add_system_to_stage(ALL, naia_client_update.system())
       .add_system_to_stage(ALL, pawn_sync.system())
       .add_system_to_stage(ALL, nonpawn_sync.system())

    // Run
       .run();
}

fn setup(mut commands: Commands, mut materials: ResMut<Assets<ColorMaterial>>) {
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
    materials: Res<Materials>,
    pawn_query: Query<(Entity, &Key, &Ref<Square>), With<Pawn>>,
    nonpawn_query: Query<(Entity, &Key), With<NonPawn>>,
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
                for (_, Key(pawn_key), _) in pawn_query.iter() {
                    if let Some(command) = queued_command.command.take() {
                        client.send_object_command(pawn_key, &command);
                    }
                }
            }
            Ok(Event::CreateObject(object_key)) => {
                if let Some(Protocol::Square(square_ref)) = client.get_object(&object_key) {
                    let square = square_ref.borrow();
                    let material = {
                        match &square.color.get() {
                            SquareColor::Red => materials.red.clone(),
                            SquareColor::Blue => materials.blue.clone(),
                            SquareColor::Yellow => materials.yellow.clone(),
                        }
                    };

                    commands
                        .spawn_bundle(SpriteBundle {
                            material: material.clone(),
                            sprite: Sprite::new(Vec2::new(SQUARE_SIZE, SQUARE_SIZE)),
                            transform: Transform::from_xyz(
                                f32::from(*(square.x.get())),
                                f32::from(*(square.y.get())) * -1.0,
                                0.0,
                            ),
                            ..Default::default()
                        })
                        .insert(Ref::clone(&square_ref))
                        .insert(NonPawn)
                        .insert(Key(object_key));
                }
            }
            Ok(Event::DeleteObject(object_key, _)) => {
                for (entity, Key(square_key)) in nonpawn_query.iter() {
                    if object_key == *square_key {
                        commands.entity(entity).despawn();
                    }
                }
            }
            Ok(Event::AssignPawn(object_key)) => {
                info!("assign pawn");

                if let Some(Protocol::Square(square_ref)) = client.get_pawn(&object_key) {
                    let square = square_ref.borrow();

                    commands
                        .spawn_bundle(SpriteBundle {
                            material: materials.white.clone(),
                            sprite: Sprite::new(Vec2::new(SQUARE_SIZE, SQUARE_SIZE)),
                            transform: Transform::from_xyz(
                                f32::from(*(square.x.get())),
                                f32::from(*(square.y.get())) * -1.0,
                                0.0,
                            ),
                            ..Default::default()
                        })
                        .insert(Ref::clone(&square_ref))
                        .insert(Pawn)
                        .insert(Key(object_key));
                }
            }
            Ok(Event::UnassignPawn(_)) => {
                info!("unassign pawn");

                for (entity, _, _) in pawn_query.iter() {
                    commands.entity(entity).despawn();
                }
            }
            Ok(Event::NewCommand(_, Protocol::KeyCommand(key_command_ref)))
            | Ok(Event::ReplayCommand(_, Protocol::KeyCommand(key_command_ref))) => {
                for (_, _, square) in pawn_query.iter() {
                    shared_behavior::process_command(&key_command_ref, square);
                }
            }
            _ => {}
        }
    }
}

fn pawn_sync(mut query: Query<(&Pawn, &Ref<Square>, &mut Transform)>) {
    for (_, pawn_ref, mut transform) in query.iter_mut() {
        let square = pawn_ref.borrow();
        transform.translation.x = f32::from(*(square.x.get()));
        transform.translation.y = f32::from(*(square.y.get())) * -1.0;
    }
}

fn nonpawn_sync(mut query: Query<(&NonPawn, &Ref<Square>, &mut Transform)>) {
    for (_, square_ref, mut transform) in query.iter_mut() {
        let square = square_ref.borrow();
        transform.translation.x = f32::from(*(square.x.get()));
        transform.translation.y = f32::from(*(square.y.get())) * -1.0;
    }
}
