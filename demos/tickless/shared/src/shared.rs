use naia_shared::{ChannelConfig, DefaultChannels, SharedConfig, SocketConfig};

pub fn shared_config() -> SharedConfig<DefaultChannels> {
    return SharedConfig::new(
        SocketConfig::new(None, None),
        ChannelConfig::default(),
        None,
        None,
    );
}
