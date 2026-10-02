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
<<<<<<< HEAD
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

/// Encodes the ICE server URL list as a JSON array string for the miniquad
/// JS bridge: `naia_socket.js` parses it into the `RTCPeerConnection`
/// `iceServers` configuration. One string crosses the `JsObject` FFI
/// boundary instead of one argument per URL, so the bridge arity stays fixed
/// no matter how many servers are configured. Quotes, backslashes, and
/// control characters are escaped so an unusual URL cannot break the parse
/// on the far side.
#[must_use]
pub fn encode_ice_server_urls(servers: &[String]) -> String {
    let mut out = String::from("[");
    for (index, url) in servers.iter().enumerate() {
        if index > 0 {
            out.push(',');
        }
        out.push('"');
        for c in url.chars() {
            match c {
                '"' => out.push_str("\\\""),
                '\\' => out.push_str("\\\\"),
                c if (c as u32) < 0x20 => {
                    out.push_str(&format!("\\u{:04x}", c as u32));
                }
                c => out.push(c),
            }
        }
        out.push('"');
    }
    out.push(']');
    out
}

#[cfg(test)]
mod ice_server_list_encoding_tests {
    use super::encode_ice_server_urls;

    #[test]
    fn an_empty_list_encodes_as_an_empty_array() {
        assert_eq!(encode_ice_server_urls(&[]), "[]");
    }

    #[test]
    fn a_single_url_encodes_as_a_one_element_array() {
        assert_eq!(
            encode_ice_server_urls(&["stun:stun.l.google.com:19302".to_string()]),
            r#"["stun:stun.l.google.com:19302"]"#,
        );
    }

    #[test]
    fn several_urls_keep_their_order() {
        assert_eq!(
            encode_ice_server_urls(&[
                "stun:one.example:3478".to_string(),
                "turn:two.example:3478".to_string(),
            ]),
            r#"["stun:one.example:3478","turn:two.example:3478"]"#,
        );
    }

    #[test]
    fn quotes_backslashes_and_controls_are_escaped() {
        assert_eq!(
            encode_ice_server_urls(&["stun:odd\"ho\\st".to_string()]),
            r#"["stun:odd\"ho\\st"]"#,
        );
    }
}
