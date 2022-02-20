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
    upload_bandwidth_monitor: Option<BandwidthMonitor>,
    download_bandwidth_monitor: Option<BandwidthMonitor>,
}

impl Io {
    pub fn new() -> Self {
        Io {
            packet_sender: None,
            packet_receiver: None,
            upload_bandwidth_monitor: None,
            download_bandwidth_monitor: None,
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

        if let Some(monitor) = &mut self.upload_bandwidth_monitor {
            monitor.record_packet(&packet.address(), packet.payload().len());
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

        if let Some(monitor) = &mut self.download_bandwidth_monitor {
            if let Ok(Some(packet)) = &receive_result {
                monitor.record_packet(&packet.address(), packet.payload().len());
            }
        }

        return receive_result;
    }

    pub fn enable_bandwidth_monitor(&mut self, bandwidth_measure_duration: Duration) {
        self.upload_bandwidth_monitor = Some(BandwidthMonitor::new(bandwidth_measure_duration));
        self.download_bandwidth_monitor = Some(BandwidthMonitor::new(bandwidth_measure_duration));
    }

    pub fn bandwidth_monitor_enabled(&self) -> bool {
        self.upload_bandwidth_monitor.is_some() && self.download_bandwidth_monitor.is_some()
    }

    pub fn register_client(&mut self, address: &SocketAddr) {
        self.upload_bandwidth_monitor.as_mut().expect("Need to call `enable_bandwidth_monitor()` on Io before calling this").create_client(address);
        self.download_bandwidth_monitor.as_mut().expect("Need to call `enable_bandwidth_monitor()` on Io before calling this").create_client(address);
    }

    pub fn deregister_client(&mut self, address: &SocketAddr) {
        self.upload_bandwidth_monitor.as_mut().expect("Need to call `enable_bandwidth_monitor()` on Io before calling this").delete_client(address);
        self.download_bandwidth_monitor.as_mut().expect("Need to call `enable_bandwidth_monitor()` on Io before calling this").delete_client(address);
    }

    pub fn upload_bandwidth_total(&mut self) -> f32 {
        return self.upload_bandwidth_monitor.as_mut().expect("Need to call `enable_bandwidth_monitor()` on Io before calling this").total_bandwidth();
    }

    pub fn download_bandwidth_total(&mut self) -> f32 {
        return self.download_bandwidth_monitor.as_mut().expect("Need to call `enable_bandwidth_monitor()` on Io before calling this").total_bandwidth();
    }

    pub fn upload_bandwidth_to_client(&mut self, address: &SocketAddr) -> f32 {
        return self.upload_bandwidth_monitor.as_mut().expect("Need to call `enable_bandwidth_monitor()` on Io before calling this").client_bandwidth(address);
    }

    pub fn download_bandwidth_from_client(&mut self, address: &SocketAddr) -> f32 {
        return self.download_bandwidth_monitor.as_mut().expect("Need to call `enable_bandwidth_monitor()` on Io before calling this").client_bandwidth(address);
    }
}
