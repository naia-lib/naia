use std::{net::SocketAddr, time::Duration};

use naia_client_socket::IdentityReceiverResult;
use naia_shared::{
    BandwidthMonitor, BitReader, CompressionConfig, Decoder, Encoder, OutgoingPacket,
};

use crate::{
    error::NaiaClientError,
    transport::{IdentityReceiver, PacketReceiver, PacketSender, ServerAddr},
};

pub struct Io {
    authenticated: bool,
    id_receiver: Option<Box<dyn IdentityReceiver>>,
    packet_sender: Option<Box<dyn PacketSender>>,
    packet_receiver: Option<Box<dyn PacketReceiver>>,
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
        let outgoing_bandwidth_monitor = bandwidth_measure_duration.map(BandwidthMonitor::new);
        let incoming_bandwidth_monitor = bandwidth_measure_duration.map(BandwidthMonitor::new);

        let outgoing_encoder = compression_config.as_ref().and_then(|config| {
            config
                .client_to_server
                .as_ref()
                .map(|mode| Encoder::new(mode.clone()))
        });
        let incoming_decoder = compression_config.as_ref().and_then(|config| {
            config
                .server_to_client
                .as_ref()
                .map(|mode| Decoder::new(mode.clone()))
        });

        Self {
            authenticated: false,
            id_receiver: None,
            packet_sender: None,
            packet_receiver: None,
            outgoing_bandwidth_monitor,
            incoming_bandwidth_monitor,
            outgoing_encoder,
            incoming_decoder,
        }
    }

    pub fn load(
        &mut self,
        id_receiver: Box<dyn IdentityReceiver>,
        packet_sender: Box<dyn PacketSender>,
        packet_receiver: Box<dyn PacketReceiver>,
    ) {
        if self.packet_sender.is_some() {
            panic!("Packet sender/receiver already loaded! Cannot do this twice!");
        }

        self.id_receiver = Some(id_receiver);
        self.packet_sender = Some(packet_sender);
        self.packet_receiver = Some(packet_receiver);
    }

    pub fn is_loaded(&self) -> bool {
        self.packet_sender.is_some()
    }

    pub fn is_authenticated(&self) -> bool {
        self.authenticated
    }

    pub fn recv_auth(&mut self) -> IdentityReceiverResult {
        let Some(id_receiver) = self.id_receiver.as_mut() else {
            return IdentityReceiverResult::Waiting;
        };

        let id_result = id_receiver.receive();

        if let IdentityReceiverResult::Success(_) = &id_result {
            self.authenticated = true;
            self.id_receiver = None;
        }

        id_result
    }

    pub fn send_packet(&mut self, packet: OutgoingPacket) -> Result<(), NaiaClientError> {
        // get payload
        let mut payload = packet.slice();

        // Compression
        if let Some(encoder) = &mut self.outgoing_encoder {
            payload = encoder.encode(payload);
        }

        // Bandwidth monitoring
        if let Some(monitor) = &mut self.outgoing_bandwidth_monitor {
            monitor.record_packet(payload.len());
        }

        self.packet_sender
            .as_mut()
            .expect("Cannot call Client.send_packet() until you call Client.connect()!")
            .send(payload)
            .map_err(|_| NaiaClientError::SendError)
    }

    pub fn recv_reader(&mut self) -> Result<Option<BitReader>, NaiaClientError> {
        let receive_result = self
            .packet_receiver
            .as_mut()
            .expect("Cannot call Client.receive_packet() until you call Client.connect()!")
            .receive();

        if let Ok(Some(mut payload)) = receive_result {
            // Bandwidth monitoring
            if let Some(monitor) = &mut self.incoming_bandwidth_monitor {
                monitor.record_packet(payload.len());
            }

            // Decompression
            if let Some(decoder) = &mut self.incoming_decoder {
                payload = decoder.decode(payload);
            }

            Ok(Some(BitReader::new(payload)))
        } else {
            receive_result
                .map(|payload_opt| payload_opt.map(BitReader::new))
                .map_err(|_| NaiaClientError::RecvError)
        }
    }

    pub fn server_addr(&self) -> Result<SocketAddr, NaiaClientError> {
        if let Some(packet_sender) = self.packet_sender.as_ref() {
            if let ServerAddr::Found(server_addr) = packet_sender.server_addr() {
                Ok(server_addr)
            } else {
                Err(NaiaClientError::from_message("Connection has not yet been established! Make sure you call Client.connect() before calling this."))
            }
        } else {
            Err(NaiaClientError::from_message("Connection has not yet been established! Make sure you call Client.connect() before calling this."))
        }
    }

    pub fn outgoing_bandwidth(&mut self) -> f32 {
        return self
            .outgoing_bandwidth_monitor
            .as_mut()
            .expect("Need to call `enable_bandwidth_monitor()` on Io before calling this")
            .bandwidth();
    }

    pub fn incoming_bandwidth(&mut self) -> f32 {
        return self
            .incoming_bandwidth_monitor
            .as_mut()
            .expect("Need to call `enable_bandwidth_monitor()` on Io before calling this")
            .bandwidth();
    }
}
