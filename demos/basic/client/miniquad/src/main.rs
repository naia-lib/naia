use miniquad::*;
use naia_basic_client_demo_app::{App, Config};

struct Stage {
    ctx: Context,
    app: App,
}
impl EventHandlerFree for Stage {
    fn update(&mut self) {
        self.app.update();
    }

    fn draw(&mut self) {
        self.ctx.clear(Some((0., 1., 0., 1.)), None, None);
    }
}

fn main() {
    let app = App::new();
    miniquad::start(conf::Conf::default(), |ctx| {
        UserData::free(Stage { ctx, app })
    });
}
