//! Split recv/send transport halves (C.3 Phase 4 step 4-E.2a).
//!
//! Previously a single `Io` struct held both ends of the transport plus
//! the encoder/decoder + bandwidth monitors. Pipeline mode runs recv and
//! send on independent threads, so the two halves are now separate
//! structs owned by `RecvState` and `SendState` respectively.

use std::{net::SocketAddr, panic, time::Duration};

use naia_shared::{CompressionConfig, Decoder, Encoder, OutgoingPacket, OwnedBitReader};

use super::bandwidth_monitor::BandwidthMonitor;
use crate::{
    error::NaiaServerError,
    transport::{PacketReceiver, PacketSender},
};

/// Receive half of the transport — owned by the recv thread.
#[derive(Clone)]
pub struct RecvIo {
    packet_receiver: Option<Box<dyn PacketReceiver>>,
    incoming_bandwidth_monitor: Option<BandwidthMonitor>,
    incoming_decoder: Option<Decoder>,
}

/// Send half of the transport — owned by the send thread.
#[derive(Clone)]
pub struct SendIo {
    packet_sender: Option<Box<dyn PacketSender>>,
    outgoing_bandwidth_monitor: Option<BandwidthMonitor>,
    outgoing_encoder: Option<Encoder>,
    /// Bytes sent during the most recent `send_all_packets` tick.
    /// Reset at the start of each `send_all_packets` via
    /// `reset_outgoing_bytes_this_tick`, incremented in `send_packet`.
    /// Unconditionally tracked — no bandwidth monitor required.
    outgoing_bytes_this_tick: u64,
}

/// Construct a fresh recv/send pair from the bandwidth + compression config.
/// Replaces the old `Io::new` (which returned a single combined struct).
pub fn new_io_pair(
    bandwidth_measure_duration: &Option<Duration>,
    compression_config: &Option<CompressionConfig>,
) -> (RecvIo, SendIo) {
    let outgoing_bandwidth_monitor = bandwidth_measure_duration.map(BandwidthMonitor::new);
    let incoming_bandwidth_monitor = bandwidth_measure_duration.map(BandwidthMonitor::new);

    let outgoing_encoder = compression_config.as_ref().and_then(|config| {
        config
            .server_to_client
            .as_ref()
            .map(|mode| Encoder::new(mode.clone()))
    });
    let incoming_decoder = compression_config.as_ref().and_then(|config| {
        config
            .client_to_server
            .as_ref()
            .map(|mode| Decoder::new(mode.clone()))
    });

    let recv = RecvIo {
        packet_receiver: None,
        incoming_bandwidth_monitor,
        incoming_decoder,
    };
    let send = SendIo {
        packet_sender: None,
        outgoing_bandwidth_monitor,
        outgoing_encoder,
        outgoing_bytes_this_tick: 0,
    };
    (recv, send)
}

impl RecvIo {
    pub fn load(&mut self, packet_receiver: Box<dyn PacketReceiver>) {
        if self.packet_receiver.is_some() {
            panic!("Packet receiver already loaded! Cannot do this twice!");
        }
        self.packet_receiver = Some(packet_receiver);
    }

    /// Awaitable readiness of the loaded transport, if it supports
    /// event-driven signalling (in-process channel ⇒ `Some`; poll-only
    /// sockets ⇒ `None`). See [`crate::transport::PacketReceiver::readiness`].
    pub fn readiness(&self) -> Option<crate::transport::PacketReadiness> {
        self.packet_receiver.as_ref().and_then(|r| r.readiness())
    }

    pub fn recv_reader(&mut self) -> Result<Option<(SocketAddr, OwnedBitReader)>, NaiaServerError> {
        let receive_result = self
            .packet_receiver
            .as_mut()
            .expect("Cannot call Server.receive_packet() until you call Server.listen()!")
            .receive();

        match receive_result {
            Ok(Some((address, mut payload))) => {
                if let Some(monitor) = &mut self.incoming_bandwidth_monitor {
                    monitor.record_packet(&address, payload.len());
                }
                if let Some(decoder) = &mut self.incoming_decoder {
                    payload = decoder.decode(payload);
                }
                Ok(Some((address, OwnedBitReader::new(payload))))
            }
            Ok(None) => Ok(None),
            Err(_) => Err(NaiaServerError::RecvError),
        }
    }

    pub fn bandwidth_monitor_enabled(&self) -> bool {
        self.incoming_bandwidth_monitor.is_some()
    }

    pub fn register_client(&mut self, address: &SocketAddr) {
        self.incoming_bandwidth_monitor
            .as_mut()
            .expect("Need to call `enable_bandwidth_monitor()` on Io before calling this")
            .create_client(address);
    }

    pub fn deregister_client(&mut self, address: &SocketAddr) {
        self.incoming_bandwidth_monitor
            .as_mut()
            .expect("Need to call `enable_bandwidth_monitor()` on Io before calling this")
            .delete_client(address);
    }

    pub fn tick_bandwidth_monitor(&mut self) {
        if let Some(monitor) = &mut self.incoming_bandwidth_monitor {
            monitor.tick();
        }
    }

    pub fn incoming_bandwidth_total(&self) -> f32 {
        self.incoming_bandwidth_monitor
            .as_ref()
            .expect("Need to call `enable_bandwidth_monitor()` on Io before calling this")
            .total_bandwidth()
    }

    pub fn incoming_bandwidth_from_client(&self, address: &SocketAddr) -> f32 {
        self.incoming_bandwidth_monitor
            .as_ref()
            .expect("Need to call `enable_bandwidth_monitor()` on Io before calling this")
            .client_bandwidth(address)
    }
}

impl SendIo {
    pub fn load(&mut self, packet_sender: Box<dyn PacketSender>) {
        if self.packet_sender.is_some() {
            panic!("Packet sender already loaded! Cannot do this twice!");
        }
        self.packet_sender = Some(packet_sender);
    }

    pub fn is_loaded(&self) -> bool {
        self.packet_sender.is_some()
    }

    pub fn sender_cloned(&self) -> Box<dyn PacketSender> {
        if self.packet_sender.is_none() {
            panic!("Cannot call Server.sender_cloned() until you call Server.listen()!");
        }
        self.packet_sender.as_ref().unwrap().clone()
    }

    pub fn send_packet(
        &mut self,
        address: &SocketAddr,
        packet: OutgoingPacket,
    ) -> Result<(), NaiaServerError> {
        let mut payload = packet.slice();

        if let Some(encoder) = &mut self.outgoing_encoder {
            payload = encoder.encode(payload);
        }

        if let Some(monitor) = &mut self.outgoing_bandwidth_monitor {
            monitor.record_packet(address, payload.len());
        }

        self.outgoing_bytes_this_tick =
            self.outgoing_bytes_this_tick.saturating_add(payload.len() as u64);

        self.packet_sender
            .as_ref()
            .expect("Cannot call Server.send_packet() until you call Server.listen()!")
            .send(address, payload)
            .map_err(|_| NaiaServerError::SendError(*address))
    }

    pub fn bandwidth_monitor_enabled(&self) -> bool {
        self.outgoing_bandwidth_monitor.is_some()
    }

    pub fn register_client(&mut self, address: &SocketAddr) {
        self.outgoing_bandwidth_monitor
            .as_mut()
            .expect("Need to call `enable_bandwidth_monitor()` on Io before calling this")
            .create_client(address);
    }

    pub fn deregister_client(&mut self, address: &SocketAddr) {
        self.outgoing_bandwidth_monitor
            .as_mut()
            .expect("Need to call `enable_bandwidth_monitor()` on Io before calling this")
            .delete_client(address);
    }

    pub fn tick_bandwidth_monitor(&mut self) {
        if let Some(monitor) = &mut self.outgoing_bandwidth_monitor {
            monitor.tick();
        }
    }

    /// Zero out the per-tick byte counter. Called by `send_all_packets` at
    /// the start of each server tick so that `outgoing_bytes_last_tick`
    /// reflects only that tick's work.
    pub fn reset_outgoing_bytes_this_tick(&mut self) {
        self.outgoing_bytes_this_tick = 0;
    }

    /// Total bytes sent (post-compression) during the most recent
    /// `send_all_packets` call. Call AFTER the tick completes.
    pub fn outgoing_bytes_last_tick(&self) -> u64 {
        self.outgoing_bytes_this_tick
    }

    pub fn outgoing_bandwidth_total(&self) -> f32 {
        self.outgoing_bandwidth_monitor
            .as_ref()
            .expect("Need to call `enable_bandwidth_monitor()` on Io before calling this")
            .total_bandwidth()
    }

    pub fn outgoing_bandwidth_to_client(&self, address: &SocketAddr) -> f32 {
        self.outgoing_bandwidth_monitor
            .as_ref()
            .expect("Need to call `enable_bandwidth_monitor()` on Io before calling this")
            .client_bandwidth(address)
    }
}
