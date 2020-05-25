pub use gaia_socket_shared::{find_my_ip_address, StringUtils};

mod acknowledgement;
pub use acknowledgement::AckHandler;

extern crate log;