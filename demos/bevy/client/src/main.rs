#[macro_use]
extern crate cfg_if;

cfg_if! {
    if #[cfg(not(target_arch = "wasm32"))] {

        mod resources;
        mod systems;
        mod app;
        mod components;

        fn main() {
            app::run();
        }
    }
}
