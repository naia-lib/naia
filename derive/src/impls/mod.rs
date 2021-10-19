cfg_if! {
    if #[cfg(feature = "client")] {
        mod client;
        pub use self::client::{replicate::replicate_impl, protocol_type::protocol_type_impl};
    }
    else if #[cfg(feature = "server")] {
        mod server;
        pub use self::server::{replicate::replicate_impl, protocol_type::protocol_type_impl};
    }
}