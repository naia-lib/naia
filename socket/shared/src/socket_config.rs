use std::default::Default;

use super::link_conditioner_config::LinkConditionerConfig;

const DEFAULT_RTC_PATH: &str = "rtc_session";

/// Default ICE server: Google's public STUN. Usable candidates (srflx/host)
/// come from here on typical networks; override via
/// [`SocketConfig::ice_servers`] where egress policy needs it.
pub const DEFAULT_ICE_SERVER_URL: &str = "stun:stun.l.google.com:19302";

/// Contains Config properties which will be shared by Server and Client sockets
#[derive(Clone)]
pub struct SocketConfig {
    /// Configuration used to simulate network conditions
    pub link_condition: Option<LinkConditionerConfig>,
    /// The endpoint URL path to use for initiating new WebRTC sessions
    pub rtc_endpoint_path: String,
    /// ICE server URLs used to configure WebRTC peer connections (wasm
    /// client backend). Defaults to [`DEFAULT_ICE_SERVER_URL`]. Customize by
    /// assigning the field; [`SocketConfig::new`] keeps the default.
    pub ice_servers: Vec<String>,
}

impl SocketConfig {
    /// Creates a new [`SocketConfig`]
    #[must_use]
    pub fn new(
        link_condition: Option<LinkConditionerConfig>,
        rtc_endpoint_path: Option<String>,
    ) -> Self {
        let endpoint_path = {
            if let Some(path) = rtc_endpoint_path {
                path
            } else {
                DEFAULT_RTC_PATH.to_string()
            }
        };

        SocketConfig {
            link_condition,
            rtc_endpoint_path: endpoint_path,
            ice_servers: vec![DEFAULT_ICE_SERVER_URL.to_string()],
        }
    }
}

impl Default for SocketConfig {
    fn default() -> Self {
        Self {
            link_condition: None,
            rtc_endpoint_path: DEFAULT_RTC_PATH.to_string(),
            ice_servers: vec![DEFAULT_ICE_SERVER_URL.to_string()],
        }
    }
}
