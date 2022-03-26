extern crate macroquad;

use macroquad::prelude::*;

use log::LevelFilter;
use simple_logger::SimpleLogger;

mod app;
use app::App;

mod command_history;

#[macroquad::main("NaiaMacroquadDemo")]
async fn main() {
    SimpleLogger::new()
        .with_level(LevelFilter::Info)
        .init()
        .expect("A logger was already initialized");

    let mut app = App::new();

    loop {
        app.update();

        next_frame().await
    }
}
