#[macro_use]
extern crate cfg_if;

cfg_if! {
    if #[cfg(target_arch = "wasm32")] {

        mod app_loop;

        use wasm_bindgen::prelude::*;

        use naia_socket_client_demo_app::App;

        use app_loop::start_loop;

        #[wasm_bindgen(start)]
        pub fn main() -> Result<(), JsValue> {
            wasm_logger::init(wasm_logger::Config::default());

            start_loop(App::default());

            Ok(())
        }
    }
}
