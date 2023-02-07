use crate::LinkConditionerConfig;

const DEFAULT_RTC_PATH: &str = "rtc_session";

/// Contains Config properties which will be shared by Server and Client sockets
#[derive(Clone)]
pub struct SocketConfig {
    /// Configuration used to simulate network conditions
    pub link_condition: Option<LinkConditionerConfig>,
    /// The endpoint URL path to use for initiating new WebRTC sessions
    pub rtc_endpoint_path: String,
}


impl SocketConfig {
    /// Creates a new SocketConfig
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
        }
    }

    pub fn link_condition(&mut self, config: LinkConditionerConfig) -> &mut Self {
        self.link_condition = Some(config);
        self
    }

    pub fn rtc_endpoint(&mut self, path: String) -> &mut Self {
        self.rtc_endpoint_path = path;
        self
    }
}

impl Default for SocketConfig {
    fn default() -> Self {
        Self {
            link_condition: None,
            rtc_endpoint_path: DEFAULT_RTC_PATH.to_string(),
        }
    }
}