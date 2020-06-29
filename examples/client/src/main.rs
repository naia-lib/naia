#[macro_use]
extern crate cfg_if;
extern crate log;

mod app;
mod loop_native;

use app::App;

fn main() {
    // Uncomment the line below to enable logging. You don't need it if something else (e.g. quicksilver) is logging for you
    simple_logger::init_with_level(log::Level::Info).expect("A logger was already initialized");

    loop_native::start_loop(&mut App::new());
}
