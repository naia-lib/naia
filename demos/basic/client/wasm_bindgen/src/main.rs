#[macro_use]
extern crate cfg_if;
extern crate log;

mod app_loop;
use app_loop::start_loop;

use naia_basic_client_demo_app::App;

cfg_if! {
    if #[cfg(target_arch = "wasm32")] {

        fn main() {
            wasm_logger::init(wasm_logger::Config::default());

            start_loop(App::new());
        }
    } else {

        fn main() {
            simple_logger::SimpleLogger::new()
                .with_level(log::LevelFilter::Info)
                .init()
                .expect("A logger was already initialized");

            start_loop(App::new());
        }
    }
}
