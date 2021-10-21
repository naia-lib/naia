#[macro_use]
extern crate cfg_if;

extern crate log;

cfg_if! {
    if #[cfg(target_arch = "wasm32")] {

        mod app;
        mod loop_wasm;
        mod systems;

        use wasm_bindgen::prelude::*;

        #[wasm_bindgen(start)]
        pub fn main_js() {
            // Uncomment the line below to enable logging. You don't need it if something else (e.g. quicksilver) is logging for you
            wasm_logger::init(wasm_logger::Config::default());

            let app = app::App::new();

            loop_wasm::start_loop(app);
        }
    } else {}
}
