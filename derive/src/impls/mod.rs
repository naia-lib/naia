cfg_if! {
    if #[cfg(feature = "client")] {
        mod client;
        pub use client::{protocol_type, replicate};
    }
    else if #[cfg(feature = "server")] {
        mod server;
        pub use server::{protocol_type, replicate};
    }
}
