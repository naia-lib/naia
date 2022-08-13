use miniquad::*;

use naia_socket_client_demo_app::App;

struct Stage {
    app: App,
}
impl EventHandler for Stage {
    fn update(&mut self, _ctx: &mut Context) {
        self.app.update();
    }

    fn draw(&mut self, ctx: &mut Context) {
        ctx.clear(Some((0., 1., 0., 1.)), None, None);
    }
}

fn main() {
    let app = App::default();
    miniquad::start(conf::Conf::default(), |_ctx| Box::new(Stage { app }));
}
