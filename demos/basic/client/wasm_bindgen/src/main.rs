#[macro_use]
extern crate cfg_if;

cfg_if! {
    if #[cfg(not(target_arch = "wasm32"))] {
        extern crate log;

        use naia_basic_client_demo_app::App;

        mod app_loop;
        use app_loop::start_loop;

        fn main() {
            simple_logger::SimpleLogger::new()
                .with_level(log::LevelFilter::Info)
                .init()
                .expect("A logger was already initialized");

            start_loop(App::new());
        }
    }
}