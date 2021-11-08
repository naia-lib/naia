extern crate cfg_if;

extern crate log;

use log::LevelFilter;
use simple_logger::SimpleLogger;

mod app;
mod loop_native;
mod systems;

fn main() {
    // Uncomment the line below to enable logging. You don't need it if something
    // else (e.g. quicksilver) is logging for you
    SimpleLogger::new()
        .with_level(LevelFilter::Info)
        .init()
        .expect("A logger was already initialized");

    loop_native::start_loop(&mut app::App::new());
}
