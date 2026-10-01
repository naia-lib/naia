// Backend modules are declared by platform/feature so every export branch
// below (and the Random selector after it) can use them independently of
// which clock selection wins.
#[cfg(all(target_arch = "wasm32", feature = "mquad"))]
mod miniquad;
#[cfg(not(all(target_arch = "wasm32", any(feature = "wbindgen", feature = "mquad"))))]
mod native;
#[cfg(feature = "test_time")]
mod test_time;
#[cfg(all(target_arch = "wasm32", feature = "wbindgen"))]
mod wasm_bindgen;

// Instant
cfg_if! {
    // test_time first: a virtual clock overrides every live backend, on any
    // target, so virtual-time suites execute identically on host and wasm.
    if #[cfg(feature = "test_time")] {
        pub use self::test_time::instant::{Instant, TestClock};
    }
    else if #[cfg(all(target_arch = "wasm32", feature = "wbindgen"))] {
        pub use self::wasm_bindgen::instant::Instant;
        pub use self::wasm_bindgen::instant::monotonic_now_ms;
    }
    else if #[cfg(all(target_arch = "wasm32", feature = "mquad"))] {
        pub use self::miniquad::instant::Instant;
    }
    else {
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
