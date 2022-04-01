extern crate macroquad;

use macroquad::prelude::*;

mod app;
use app::App;

mod command_history;

#[macroquad::main("NaiaMacroquadDemo")]
async fn main() {
    let mut app = App::new();

    loop {
        app.update();

        next_frame().await
    }
}
