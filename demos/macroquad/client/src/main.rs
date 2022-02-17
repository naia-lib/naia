extern crate cfg_if;
extern crate macroquad;

use macroquad::prelude::*;

mod app;
mod command_history;
use app::App;

#[macroquad::main("NaiaMacroquadDemo")]
async fn main() {
    let mut app = App::new();

    loop {
        app.update();

        next_frame().await
    }
}
