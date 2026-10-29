use crate::webrtc::ice::agent::agent_config::InterfaceFilterFn;
use crate::webrtc::ice::error::*;
use crate::webrtc::ice::network_type::*;

use crate::webrtc::stun::{attributes::*, integrity::*, message::*, textattrs::*};
use crate::webrtc::util::{vnet::net::*, Conn};
use std::collections::HashSet;
use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;

pub(crate) fn create_addr(_network: NetworkType, ip: IpAddr, port: u16) -> SocketAddr {
    /*if network.is_tcp(){
        return &net.TCPAddr{IP: ip, Port: port}
    default:
        return &net.UDPAddr{IP: ip, Port: port}
    }*/
    SocketAddr::new(ip, port)
}

pub(crate) fn assert_inbound_username(m: &Message, expected_username: &str) -> Result<()> {
    let mut username = Username::new(ATTR_USERNAME, String::new());
    username.get_from(m)?;

    if username.to_string() != expected_username {
        return Err(Error::Other(format!(
            "{:?} expected({}) actual({})",
            Error::ErrMismatchUsername,
            expected_username,
            username,
        )));
    }

    Ok(())
}

pub(crate) fn assert_inbound_message_integrity(m: &mut Message, key: &[u8]) -> Result<()> {
    let message_integrity_attr = MessageIntegrity(key.to_vec());
    Ok(message_integrity_attr.check(m)?)
}

pub(crate) async fn local_interfaces(
    vnet: &Arc<Net>,
    interface_filter: &Option<InterfaceFilterFn>,
    network_types: &[NetworkType],
) -> HashSet<IpAddr> {
    let mut ips = HashSet::new();
    let interfaces = vnet.get_interfaces().await;

    let (mut ipv4requested, mut ipv6requested) = (false, false);
    for typ in network_types {
        if typ.is_ipv4() {
            ipv4requested = true;
        }
        if typ.is_ipv6() {
            ipv6requested = true;
        }
    }

    for iface in interfaces {
        if let Some(filter) = interface_filter {
            if !filter(iface.name()) {
                continue;
            }
        }

        for ipnet in iface.addrs() {
            let ipaddr = ipnet.addr();
            if ipv4requested && ipaddr.is_ipv4() || ipv6requested && ipaddr.is_ipv6() {
                ips.insert(ipaddr);
            }
        }
    }

    ips
}

pub(crate) async fn listen_udp_in_port_range(
    vnet: &Arc<Net>,
    laddr: SocketAddr,
) -> Result<Arc<dyn Conn + Send + Sync>> {
    return Ok(vnet.bind(laddr).await?);
}
