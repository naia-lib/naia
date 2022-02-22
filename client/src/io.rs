use std::{net::SocketAddr, time::Duration};

use naia_client_socket::{NaiaClientSocketError, Packet, PacketReceiver, PacketSender, ServerAddr};
pub use naia_shared::{
    ConnectionConfig, ManagerType, Manifest, PacketReader, PacketType, ProtocolKindType,
    Protocolize, ReplicateSafe, SharedConfig, StandardHeader, Timer, Timestamp, WorldMutType,
    WorldRefType, BandwidthMonitor, CompressionManager, CompressionConfig
};

pub struct Io {
    packet_sender: Option<PacketSender>,
    packet_receiver: Option<PacketReceiver>,
    upload_bandwidth_monitor: Option<BandwidthMonitor>,
    download_bandwidth_monitor: Option<BandwidthMonitor>,
    upload_compression_manager: Option<CompressionManager>,
    download_compression_manager: Option<CompressionManager>,
}

impl Io {
    pub fn new(bandwidth_measure_duration: &Option<Duration>, compression_config: &Option<CompressionConfig>) -> Self {

        let upload_bandwidth_monitor = bandwidth_measure_duration.map(|duration| BandwidthMonitor::new(duration));
        let download_bandwidth_monitor = bandwidth_measure_duration.map(|duration| BandwidthMonitor::new(duration));

        let upload_compression_manager = compression_config.as_ref().map(|config| config.client_to_server.map(|_| CompressionManager::new())).flatten();
        let download_compression_manager = compression_config.as_ref().map(|config| config.server_to_client.map(|_| CompressionManager::new())).flatten();

        Io {
            packet_sender: None,
            packet_receiver: None,
            upload_bandwidth_monitor,
            download_bandwidth_monitor,
            upload_compression_manager,
            download_compression_manager,
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

    pub fn send_packet(&mut self, mut packet: Packet) {

        // Compression
        if let Some(compression_manager) = &mut self.upload_compression_manager {
            packet = Packet::new_raw(compression_manager.compress(packet.payload()).into());
        }

        // Bandwidth monitoring
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

        if let Ok(Some(mut packet)) = receive_result  {

            // Compression
            if let Some(compression_manager) = &mut self.download_compression_manager {
                packet = Packet::new_raw(compression_manager.decompress(packet.payload()).into());
            }

            // Bandwidth monitoring
            if let Some(monitor) = &mut self.download_bandwidth_monitor {
                monitor.record_packet(packet.payload().len());
            }

            return Ok(Some(packet));
        } else {
            return receive_result;
        }
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
