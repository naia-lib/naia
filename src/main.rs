
extern crate log;

mod app;
mod loop_native;

use log::info;
use app::App;

fn main() {
    // Uncomment the line below to enable logging. You don't need it if something else (e.g. quicksilver) is logging for you
    simple_logger::init_with_level(log::Level::Info).expect("A logger was already initialized");

    info!("Gaia Client Example Started");

    let mut app = App::new();

    loop_native::start_loop(&mut app);
}