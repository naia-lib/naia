use std::{net::SocketAddr, time::Duration};

use naia_client_socket::{NaiaClientSocketError, Packet, PacketReceiver, PacketSender, ServerAddr};
pub use naia_shared::{
    ConnectionConfig, ManagerType, Manifest, PacketReader, PacketType, ProtocolKindType,
    Protocolize, ReplicateSafe, SharedConfig, StandardHeader, Timer, Timestamp, WorldMutType,
    WorldRefType, BandwidthMonitor
};

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
            monitor.record_packet(packet.payload().len());
        }

        self.packet_sender
            .as_mut()
            .expect("Cannot call Client.send_packet() until you call Client.connect()!")
            .send(packet);
    }

    pub fn receive_packet(&mut self) -> Result<Option<Packet>, NaiaClientSocketError> {

        let receive_result = self
            .packet_receiver
            .as_mut()
            .expect("Cannot call Client.receive_packet() until you call Client.connect()!")
            .receive();

        if let Some(monitor) = &mut self.download_bandwidth_monitor {
            if let Ok(Some(packet)) = &receive_result {
                monitor.record_packet(packet.payload().len());
            }
        }

        return receive_result;
    }

    pub fn server_addr_unwrapped(&self) -> SocketAddr {
        if let ServerAddr::Found(server_addr) = self
            .packet_sender
            .as_ref()
            .expect("Cannot call Client.server_addr_unwrapped() until you call Client.connect()!")
            .server_addr()
        {
            return server_addr;
        } else {
            panic!("Connection has not yet been established! Call server_addr() instead when unsure about the connection status.")
        }
    }

    pub fn enable_bandwidth_monitor(&mut self, bandwidth_measure_duration: Duration) {
        self.upload_bandwidth_monitor = Some(BandwidthMonitor::new(bandwidth_measure_duration));
        self.download_bandwidth_monitor = Some(BandwidthMonitor::new(bandwidth_measure_duration));
    }

    pub fn upload_bandwidth(&mut self) -> f32 {
        return self.upload_bandwidth_monitor
            .as_mut()
            .expect("Need to call `enable_bandwidth_monitor()` on Io before calling this")
            .bandwidth();
    }

    pub fn download_bandwidth(&mut self) -> f32 {
        return self.download_bandwidth_monitor
            .as_mut()
            .expect("Need to call `enable_bandwidth_monitor()` on Io before calling this")
            .bandwidth();
    }
}
