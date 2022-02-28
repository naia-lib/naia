use std::{net::SocketAddr, panic, time::Duration};

use naia_server_socket::{NaiaServerSocketError, Packet, PacketReceiver, PacketSender};
pub use naia_shared::{
    wrapping_diff, BaseConnection, CompressionConfig, ConnectionConfig, Decoder, Encoder, Instant,
    KeyGenerator, LocalComponentKey, ManagerType, Manifest, PacketType,
    PropertyMutate, PropertyMutator, ProtocolKindType, Protocolize, Replicate, ReplicateSafe,
    SharedConfig, StandardHeader, Timer, Timestamp, WorldMutType, WorldRefType,
};
use naia_shared::serde::{BitWriter, OwnedBitReader};

use crate::bandwidth_monitor::BandwidthMonitor;

pub struct Io {
    packet_sender: Option<PacketSender>,
    packet_receiver: Option<PacketReceiver>,
    outgoing_bandwidth_monitor: Option<BandwidthMonitor>,
    incoming_bandwidth_monitor: Option<BandwidthMonitor>,
    outgoing_encoder: Option<Encoder>,
    incoming_decoder: Option<Decoder>,
}

impl Io {
    pub fn new(
        bandwidth_measure_duration: &Option<Duration>,
        compression_config: &Option<CompressionConfig>,
    ) -> Self {
        let outgoing_bandwidth_monitor =
            bandwidth_measure_duration.map(|duration| BandwidthMonitor::new(duration));
        let incoming_bandwidth_monitor =
            bandwidth_measure_duration.map(|duration| BandwidthMonitor::new(duration));

        let outgoing_encoder = compression_config
            .as_ref()
            .map(|config| {
                config
                    .server_to_client
                    .as_ref()
                    .map(|mode| Encoder::new(mode.clone()))
            })
            .flatten();
        let incoming_decoder = compression_config
            .as_ref()
            .map(|config| {
                config
                    .client_to_server
                    .as_ref()
                    .map(|mode| Decoder::new(mode.clone()))
            })
            .flatten();

        Io {
            packet_sender: None,
            packet_receiver: None,
            outgoing_bandwidth_monitor,
            incoming_bandwidth_monitor,
            outgoing_encoder,
            incoming_decoder,
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

    pub fn send_writer(&mut self, address: &SocketAddr, writer: &mut BitWriter) {

        // get payload
        let (length, buffer) = writer.flush();
        let mut payload = &buffer[0..length];

        // Compression
        if let Some(encoder) = &mut self.outgoing_encoder {
            payload = encoder.encode(&payload);
        }

        // Bandwidth monitoring
        if let Some(monitor) = &mut self.outgoing_bandwidth_monitor {
            monitor.record_packet(address, payload.len());
        }

        self.packet_sender
            .as_ref()
            .expect("Cannot call Server.send_packet() until you call Server.listen()!")
            .send(Packet::new(*address, payload.into()));
    }

    pub fn recv_reader(&mut self) -> Result<Option<(SocketAddr, OwnedBitReader)>, NaiaServerSocketError> {
        let receive_result = self
            .packet_receiver
            .as_mut()
            .expect("Cannot call Server.receive_packet() until you call Server.listen()!")
            .receive();

        if let Ok(Some(mut packet)) = receive_result {
            // Bandwidth monitoring
            if let Some(monitor) = &mut self.incoming_bandwidth_monitor {
                monitor.record_packet(&packet.address, packet.payload.len());
            }

            // Decompression
            if let Some(decoder) = &mut self.incoming_decoder {
                packet = Packet::new(packet.address, decoder.decode(&packet.payload).into());
            }

            return Ok(Some((packet.address.clone(), OwnedBitReader::new(packet.payload))));
        } else {
            return receive_result.map(|packet_opt| packet_opt.map(|packet| (packet.address.clone(), OwnedBitReader::new(packet.payload))));
        }
    }

    pub fn bandwidth_monitor_enabled(&self) -> bool {
        self.outgoing_bandwidth_monitor.is_some() && self.incoming_bandwidth_monitor.is_some()
    }

    pub fn register_client(&mut self, address: &SocketAddr) {
        self.outgoing_bandwidth_monitor
            .as_mut()
            .expect("Need to call `enable_bandwidth_monitor()` on Io before calling this")
            .create_client(address);
        self.incoming_bandwidth_monitor
            .as_mut()
            .expect("Need to call `enable_bandwidth_monitor()` on Io before calling this")
            .create_client(address);
    }

    pub fn deregister_client(&mut self, address: &SocketAddr) {
        self.outgoing_bandwidth_monitor
            .as_mut()
            .expect("Need to call `enable_bandwidth_monitor()` on Io before calling this")
            .delete_client(address);
        self.incoming_bandwidth_monitor
            .as_mut()
            .expect("Need to call `enable_bandwidth_monitor()` on Io before calling this")
            .delete_client(address);
    }

    pub fn outgoing_bandwidth_total(&mut self) -> f32 {
        return self
            .outgoing_bandwidth_monitor
            .as_mut()
            .expect("Need to call `enable_bandwidth_monitor()` on Io before calling this")
            .total_bandwidth();
    }

    pub fn incoming_bandwidth_total(&mut self) -> f32 {
        return self
            .incoming_bandwidth_monitor
            .as_mut()
            .expect("Need to call `enable_bandwidth_monitor()` on Io before calling this")
            .total_bandwidth();
    }

    pub fn outgoing_bandwidth_to_client(&mut self, address: &SocketAddr) -> f32 {
        return self
            .outgoing_bandwidth_monitor
            .as_mut()
            .expect("Need to call `enable_bandwidth_monitor()` on Io before calling this")
            .client_bandwidth(address);
    }

    pub fn incoming_bandwidth_from_client(&mut self, address: &SocketAddr) -> f32 {
        return self
            .incoming_bandwidth_monitor
            .as_mut()
            .expect("Need to call `enable_bandwidth_monitor()` on Io before calling this")
            .client_bandwidth(address);
    }
}
