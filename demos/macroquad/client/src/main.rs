use macroquad::prelude::*;

mod app;
use app::App;

#[macroquad::main("NaiaMacroquadDemo")]
async fn main() {
    let mut app = App::default();

    loop {
        app.update();

        next_frame().await
    }
}
