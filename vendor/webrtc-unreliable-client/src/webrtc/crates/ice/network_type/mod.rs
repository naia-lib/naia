use crate::webrtc::ice::error::*;

use std::fmt;
use std::net::IpAddr;

pub(crate) const UDP: &str = "udp";

#[must_use]
pub(crate) fn supported_network_types() -> Vec<NetworkType> {
    vec![NetworkType::Udp4, NetworkType::Udp6]
}

/// Represents the type of network.
#[derive(PartialEq, Debug, Copy, Clone, Eq, Hash, Default)]
pub(crate) enum NetworkType {
    #[default]
    Unspecified,

    /// Indicates UDP over IPv4.
    Udp4,

    /// Indicates UDP over IPv6.
    Udp6,
}

impl From<u8> for NetworkType {
    fn from(v: u8) -> Self {
        match v {
            1 => Self::Udp4,
            2 => Self::Udp6,
            _ => Self::Unspecified,
        }
    }
}

impl fmt::Display for NetworkType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match *self {
            Self::Udp4 => "udp4",
            Self::Udp6 => "udp6",
            Self::Unspecified => "unspecified",
        };
        write!(f, "{}", s)
    }
}

impl NetworkType {
    /// Returns the short network description.
    #[must_use]
    pub(crate) fn network_short(self) -> String {
        match self {
            Self::Udp4 | Self::Udp6 => UDP.to_owned(),
            Self::Unspecified => "Unspecified".to_owned(),
        }
    }

    /// Returns whether the network type is IPv4 or not.
    #[must_use]
    pub(crate) const fn is_ipv4(self) -> bool {
        match self {
            Self::Udp4 => true,
            Self::Udp6 | Self::Unspecified => false,
        }
    }

    /// Returns whether the network type is IPv6 or not.
    #[must_use]
    pub(crate) const fn is_ipv6(self) -> bool {
        match self {
            Self::Udp6 => true,
            Self::Udp4 | Self::Unspecified => false,
        }
    }
}

/// Determines the type of network based on the short network string and an IP address.
pub(crate) fn determine_network_type(network: &str, ip: &IpAddr) -> Result<NetworkType> {
    let ipv4 = ip.is_ipv4();
    let net = network.to_lowercase();
    if net.starts_with(UDP) {
        if ipv4 {
            Ok(NetworkType::Udp4)
        } else {
            Ok(NetworkType::Udp6)
        }
    } else {
        Err(Error::ErrDetermineNetworkType)
    }
}
