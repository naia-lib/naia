use naia_shared::{SharedConfig, SocketConfig};

use super::protocol::Protocol;

pub fn shared_config() -> SharedConfig<Protocol> {
    return SharedConfig::new(
        Protocol::load(),
        SocketConfig::new(None, None),
        None,
        None,
        None,
    );
}
