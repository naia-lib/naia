pub mod shared;

cfg_if! {
    if #[cfg(feature = "client")] {
        mod client;
        pub use self::client::replicate_impl;
    }
    else if #[cfg(feature = "server")] {
        mod server;
        pub use self::server::replicate_impl;
    }
}