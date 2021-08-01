extern crate cfg_if;

mod app;
use app::App;

use macroquad::prelude::*;

#[macroquad::main("NaiaMacroquadExample")]
async fn main() {
    let mut app = App::new();

    loop {
        clear_background(BLACK);

        app.update();

        next_frame().await
    }
}
