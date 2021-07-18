use naia_client_example_app::App;

pub fn start_loop(app: &mut App) {
    loop {
        app.update();
    }
}
