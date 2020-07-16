use std::time::Duration;

use naia_shared::SharedConfig;

pub fn get_shared_config() -> SharedConfig {
    let tick_interval = Duration::from_millis(50);
    return SharedConfig::new(tick_interval);
}
