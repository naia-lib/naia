#[macro_use]
extern crate log;
extern crate naia_macroquad_demo_shared;

use log::LevelFilter;
use simple_logger::SimpleLogger;

mod app;
use app::App;

fn main() {
    SimpleLogger::new()
        .with_level(LevelFilter::Info)
        .init()
        .expect("A logger was already initialized");

    let mut app = App::default();
    loop {
        app.update();
    }
}
