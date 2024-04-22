cfg_if! {
    if #[cfg(feature = "advanced_handshake")] {
        mod advanced;
        pub use advanced::*;
    } else {
        mod simple;
        pub use simple::*;
    }
}
