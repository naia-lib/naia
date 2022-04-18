cfg_if! {
    if #[cfg(all(target_arch = "wasm32", feature = "wbindgen"))] {
        mod wasm_bindgen;
        pub use self::wasm_bindgen::random::Random;
        pub use self::wasm_bindgen::instant::Instant;
    }
    else if #[cfg(all(target_arch = "wasm32", feature = "mquad"))] {
        mod miniquad;
        pub use self::miniquad::random::Random;
        pub use self::miniquad::instant::Instant;
    }
    else {
        mod native;
        pub use native::random::Random;
        pub use native::instant::Instant;
    }
}
