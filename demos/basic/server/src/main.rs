#[macro_use]
extern crate log;

use log::LevelFilter;
use simple_logger::SimpleLogger;
use smol::io;

use naia_basic_demo_shared::protocol::Protocol;

mod app;
use app::App;

fn main() -> io::Result<()> {
    SimpleLogger::new()
        .with_level(LevelFilter::Info)
        .init()
        .expect("A logger was already initialized");

    let mut app = App::new::<Protocol>();
    loop {
        app.update();
    }
}
