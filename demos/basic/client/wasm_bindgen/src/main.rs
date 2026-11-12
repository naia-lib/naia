#[macro_use]
extern crate cfg_if;

cfg_if! {
    if #[cfg(not(target_arch = "wasm32"))] {

        extern crate log;

        mod app_loop;

        use naia_basic_client_demo_app::App;

        use app_loop::start_loop;

        fn main() -> Result<(), log::SetLoggerError> {
            simple_logger::SimpleLogger::new()
                .with_level(log::LevelFilter::Info)
                .init()?;

            start_loop(App::default());
            Ok(())
        }
    }
}
