#[macro_use]
extern crate cfg_if;

cfg_if! {
    if #[cfg(target_arch = "wasm32")] {

        mod app;
        mod loop_wasm;

        use wasm_bindgen::prelude::*;

        use crate::app::App;

        #[wasm_bindgen(start)]
        pub fn main_js() {
            // Uncomment the line below to enable logging. You don't need it if something else (e.g. quicksilver) is logging for you
            web_logger::custom_init(web_logger::Config { level: log::Level::Info });

            let app = App::new();

            loop_wasm::start_loop(app);
        }
    } else {}
}
