cfg_if! {
    if #[cfg(feature = "client")] {
        mod client;
        pub use client::protocol_type;
        pub use self::client::replicate::replicate_impl;
    }
    else if #[cfg(feature = "server")] {
        mod server;
        pub use server::protocol_type;
        pub use self::server::replicate::replicate_impl;
    }
}