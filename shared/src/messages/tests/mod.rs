// proptest (via rusty-fork/wait-timeout/getrandom) has no
// wasm32-unknown-unknown support, so this strategy suite stays on native.
// The dev-dependency itself is target-gated in shared/Cargo.toml.
#[cfg(not(target_arch = "wasm32"))]
mod channel_ordering;
mod fragment;
