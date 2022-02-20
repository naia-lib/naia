use std::{net::SocketAddr, panic, time::Duration};

use naia_server_socket::{NaiaServerSocketError, Packet, PacketReceiver, PacketSender};
pub use naia_shared::{
    wrapping_diff, BaseConnection, ConnectionConfig, Instant, KeyGenerator, LocalComponentKey,
    ManagerType, Manifest, PacketReader, PacketType, PropertyMutate, PropertyMutator,
    ProtocolKindType, Protocolize, Replicate, ReplicateSafe, SharedConfig, StandardHeader, Timer,
    Timestamp, WorldMutType, WorldRefType,
};

use crate::bandwidth_monitor::BandwidthMonitor;

pub struct Io {
    packet_sender: Option<PacketSender>,
    packet_receiver: Option<PacketReceiver>,
    bandwidth_monitor: Option<BandwidthMonitor>,
}

impl Io {
    pub fn new() -> Self {
        Io {
            packet_sender: None,
            packet_receiver: None,
            bandwidth_monitor: None,
        }
    }

    pub fn load(&mut self, packet_sender: PacketSender, packet_receiver: PacketReceiver) {
        if self.packet_sender.is_some() {
            panic!("Packet sender/receiver already loaded! Cannot do this twice!");
        }

        self.packet_sender = Some(packet_sender);
        self.packet_receiver = Some(packet_receiver);
    }

    pub fn is_loaded(&self) -> bool {
        self.packet_sender.is_some()
    }

    pub fn send_packet(&mut self, packet: Packet) {

        if let Some(monitor) = &mut self.bandwidth_monitor {
            monitor.send_packet(&packet.address(), packet.payload().len());
        }

        self.packet_sender
            .as_ref()
            .expect("Cannot call Server.send_packet() until you call Server.listen()!")
            .send(packet);
    }

    pub fn receive_packet(&mut self) -> Result<Option<Packet>, NaiaServerSocketError> {

        let receive_result = self
            .packet_receiver
            .as_mut()
            .expect("Cannot call Server.receive_packet() until you call Server.listen()!")
            .receive();

        if let Some(monitor) = &mut self.bandwidth_monitor {
            if let Ok(Some(packet)) = &receive_result {
                monitor.receive_packet(&packet.address(), packet.payload().len());
            }
        }

        return receive_result;
    }

    pub fn enable_bandwidth_monitor(&mut self, bandwidth_measure_duration: Duration) {
        self.bandwidth_monitor = Some(BandwidthMonitor::new(bandwidth_measure_duration));
    }

    pub fn upload_bandwidth_total(&self) -> f32 {
        return self.bandwidth_monitor.as_ref().expect("Need to call `enable_bandwidth_monitor()` on Io before calling this").upload_bandwidth_total();
    }

    pub fn download_bandwidth_total(&self) -> f32 {
        return self.bandwidth_monitor.as_ref().expect("Need to call `enable_bandwidth_monitor()` on Io before calling this").download_bandwidth_total();
    }

    pub fn upload_bandwidth_to_client(&self, address: &SocketAddr) -> f32 {
        return self.bandwidth_monitor.as_ref().expect("Need to call `enable_bandwidth_monitor()` on Io before calling this").upload_bandwidth_to_client(address);
    }

    pub fn download_bandwidth_from_client(&self, address: &SocketAddr) -> f32 {
        return self.bandwidth_monitor.as_ref().expect("Need to call `enable_bandwidth_monitor()` on Io before calling this").download_bandwidth_from_client(address);
    }
}
