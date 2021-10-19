cfg_if! {
    if #[cfg(feature = "client")] {
        mod client;
        pub use self::client::{
            property::Property,
            replicate::{Replicate, ReplicateEq},
            protocol_type::ProtocolType,
        };
    }
    else if #[cfg(feature = "server")] {
        mod server;
        pub use self::server::{
            property::Property,
            replicate::{Replicate, ReplicateEq},
            protocol_type::ProtocolType,
        };
    }
}
