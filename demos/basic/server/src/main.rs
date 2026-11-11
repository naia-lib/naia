#[macro_use]
extern crate log;

use log::LevelFilter;
use simple_logger::SimpleLogger;
use smol::io;

mod app;
use app::App;

fn main() -> io::Result<()> {
    SimpleLogger::new()
        .with_level(LevelFilter::Info)
        .init()
        .map_err(io::Error::other)?;

    let mut app = App::new();
    loop {
        app.update();
    }
}
