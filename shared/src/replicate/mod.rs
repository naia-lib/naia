cfg_if! {
    if #[cfg(feature = "client")] {
        mod client;
        pub use self::client::{Replicate, ReplicateEq};
    }
    else if #[cfg(feature = "server")] {
        mod server;
        pub use self::server::{Replicate, ReplicateEq};
    }
}