use bevy::prelude::*;

use naia_bevy_client::{ClientConfig, Plugin as ClientPlugin, Ref};

use naia_bevy_demo_shared::{
    get_server_address, get_shared_config,
    protocol::{Auth, Position},
};

mod components;
mod resources;
mod systems;

use components::{Confirmed, Predicted};
use systems::{init, player_input, receive_events, should_tick, tick};

fn main() {
    let mut app = App::build();

    // Plugins
    app.add_plugins(DefaultPlugins)
        .add_plugin(ClientPlugin::new(
            ClientConfig::default(),
            get_shared_config(),
            get_server_address(),
            Some(Auth::new("charlie", "12345")),
        ));

    #[cfg(target_arch = "wasm32")]
    app.add_plugin(bevy_webgl2::WebGL2Plugin);

    app
    // Startup System
    .add_startup_system(
        init.system())
    // Receive Server Events
    .add_system_to_stage(
        CoreStage::PreUpdate,
        player_input.system())
    // Realtime Gameplay Loop
    .add_system_to_stage(
        CoreStage::Update,
        receive_events.system())
    .add_system_to_stage(
        CoreStage::Update,
        predicted_sync.system())
    .add_system_to_stage(
        CoreStage::Update,
        confirmed_sync.system())
    // Gameplay Loop on Tick
    .add_system_to_stage(
        CoreStage::PostUpdate,
        tick.system()
            .with_run_criteria(
                should_tick.system()))

    // Run App
    .run();

    //    let mut client = Client::new(ClientConfig::default(),
    // get_shared_config());    client.connect(server_address, auth);
    //
    //    // Add Naia Client
    //    app.insert_non_send_resource(client);
    //
    //    // Resources
    //    app.insert_non_send_resource(QueuedCommand { command: None })
    //        .insert_resource(ClearColor(Color::rgb(0.0, 0.0, 0.0)))
    //        .insert_resource(ClientResource {
    //            entity_key_map: HashMap::new(),
    //            prediction_key_map: HashMap::new(),
    //        });
    //
    //    // Systems
    //    app.add_startup_system(init.system())
    //       .add_system(prediction_input.system())
    //       .add_system_to_stage(ALL, naia_client_update.system())
    //       .add_system_to_stage(ALL, predicted_sync.system())
    //       .add_system_to_stage(ALL, confirmed_sync.system())
    //
    //    // Run
    //       .run();
}

fn predicted_sync(mut query: Query<(&Predicted, &Ref<Position>, &mut Transform)>) {
    for (_, pos_ref, mut transform) in query.iter_mut() {
        let pos = pos_ref.borrow();
        transform.translation.x = f32::from(*(pos.x.get()));
        transform.translation.y = f32::from(*(pos.y.get())) * -1.0;
    }
}

fn confirmed_sync(mut query: Query<(&Confirmed, &Ref<Position>, &mut Transform)>) {
    for (_, pos_ref, mut transform) in query.iter_mut() {
        let pos = pos_ref.borrow();
        transform.translation.x = f32::from(*(pos.x.get()));
        transform.translation.y = f32::from(*(pos.y.get())) * -1.0;
    }
}
