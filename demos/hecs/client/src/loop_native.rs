use naia_demo_basic_client_app::App;

pub fn start_loop(app: &mut App) {
    loop {
        app.update();
    }
}
