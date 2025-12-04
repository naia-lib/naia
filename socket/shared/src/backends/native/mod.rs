pub mod random;

cfg_if! {
    if #[cfg(not(feature = "test_time"))] {
        pub mod instant;
    }
}