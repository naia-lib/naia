
#[macro_use]
extern crate cfg_if;

cfg_if! {
    if #[cfg(target_arch = "wasm32")] {

        use log::{info};
        mod app;
        mod loop_wasm;

        use wasm_bindgen::prelude::*;

        use crate::app::App;

        const SERVER_IP_ADDRESS: &str = "192.168.1.9"; // Put your Server's IP Address here!, can't easily find this automatically from the browser
        const SERVER_PORT: &str = "3179";

        #[wasm_bindgen(start)]
        pub fn main_js() {
            // Uncomment the line below to enable logging. You don't need it if something else (e.g. quicksilver) is logging for you
            web_logger::custom_init(web_logger::Config { level: log::Level::Info });

            info!("Naia Client Example Started");

            let server_socket_address = SERVER_IP_ADDRESS.to_owned() + ":" + SERVER_PORT;

            let app = App::new(&server_socket_address);

            loop_wasm::start_loop(app);
        }
    } else {}
}