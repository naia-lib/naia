// Instant
cfg_if! {
    if #[cfg(all(target_arch = "wasm32", feature = "wbindgen"))] {
        mod wasm_bindgen;
        pub use self::wasm_bindgen::instant::Instant;
    }
    else if #[cfg(all(target_arch = "wasm32", feature = "mquad"))] {
        mod miniquad;
        pub use self::miniquad::instant::Instant;
    }
    else if #[cfg(feature = "test_time")] {
        mod native;
        mod test_time;
        pub use self::test_time::instant::{Instant, TestClock};
    }
    else {
        mod native;
        pub use native::instant::Instant;
    }
}

// Random
cfg_if! {
    if #[cfg(all(target_arch = "wasm32", feature = "wbindgen"))] {
        pub use self::wasm_bindgen::random::Random;
    }
    else if #[cfg(all(target_arch = "wasm32", feature = "mquad"))] {
        pub use self::miniquad::random::Random;
    }
    else {
        pub use native::random::Random;
    }
}
