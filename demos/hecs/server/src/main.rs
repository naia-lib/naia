#[macro_use]
extern crate log;

use log::LevelFilter;
use simple_logger::SimpleLogger;
use smol::io;

mod app;
mod systems;

use app::App;

fn main() -> io::Result<()> {
    SimpleLogger::new()
        .with_level(LevelFilter::Info)
        .init()
        .expect("A logger was already initialized");

    let mut app = App::default();
    loop {
        app.update();
    }
}
