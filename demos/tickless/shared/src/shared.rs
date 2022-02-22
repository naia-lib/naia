use naia_shared::{LinkConditionerConfig, SharedConfig};

use super::protocol::Protocol;

pub fn shared_config() -> SharedConfig<Protocol> {
    let tick_interval = None;

    let link_condition = None;

    return SharedConfig::new(Protocol::load(), tick_interval, link_condition, None, None);
}
