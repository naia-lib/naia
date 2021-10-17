use nanoserde::{DeBin, SerBin};

use naia_socket_shared::PacketReader;

use super::{property_mutate::PropertyMutate, wrapping_number::sequence_greater_than};

cfg_if! {
    if #[cfg(feature = "client")] {
        mod client;
        pub use self::client::Property;
    }
    else if #[cfg(feature = "server")] {
        mod server;
        pub use self::server::Property;
    }
}