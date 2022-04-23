#[macro_use]
extern crate log;

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
