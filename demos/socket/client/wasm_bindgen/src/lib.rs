#[macro_use]
extern crate cfg_if;

mod app_loop;

use app_loop::start_loop;
use naia_socket_client_demo_app::App;

cfg_if! {
    if #[cfg(target_arch = "wasm32")] {

        use wasm_bindgen::prelude::*;

        #[wasm_bindgen(start)]
        pub fn main() -> Result<(), JsValue> {
            wasm_logger::init(wasm_logger::Config::default());

            start_loop(App::new());

            Ok(())
        }
    }
}


