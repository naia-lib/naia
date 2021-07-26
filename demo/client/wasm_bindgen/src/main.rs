#[macro_use]
extern crate cfg_if;

extern crate log;

use naia_client_example_app::App;
use simple_logger::SimpleLogger;
use log::LevelFilter;

mod loop_native;

fn main() {
    // Uncomment the line below to enable logging. You don't need it if something
    // else (e.g. quicksilver) is logging for you
    SimpleLogger::new()
            .with_level(LevelFilter::Info)
            .init()
            .expect("A logger was already initialized");

    loop_native::start_loop(&mut App::new());
}
