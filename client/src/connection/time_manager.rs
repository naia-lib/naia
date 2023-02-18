use naia_shared::{BitReader, BitWriter, PacketType, PingIndex, PingStore, Serde, SerdeErr, StandardHeader, Tick, Timer};
use std::time::Duration;
use log::{info, warn};
use crate::connection::io::Io;

use crate::connection::time_config::TimeConfig;

const HANDSHAKE_PONGS_REQUIRED: u8 = 20;

/// Is responsible for sending regular ping messages between client/servers
/// and to estimate rtt/jitter
pub struct TimeManager {
    pub rtt: f32,
    pub jitter: f32,
    pub tick_duration: f32,
    ping_timer: Timer,
    sent_pings: PingStore,
    handshake_finished: bool,
    handshake_pongs_received: u8,
}

impl TimeManager {
    pub fn new(time_config: &TimeConfig, tick_duration: &Duration) -> Self {
        let rtt_average = time_config.rtt_initial_estimate.as_secs_f32() * 1000.0;
        let jitter_average = time_config.jitter_initial_estimate.as_secs_f32() * 1000.0;
        let tick_duration_average = tick_duration.as_secs_f32() * 1000.0;

        TimeManager {
            rtt: rtt_average,
            jitter: jitter_average,
            tick_duration: tick_duration_average,
            ping_timer: Timer::new(time_config.ping_interval),
            sent_pings: PingStore::new(),
            handshake_pongs_received: 0,
            handshake_finished: false,
        }
    }

    pub(crate) fn handshake_finished(&self) -> bool {
        self.handshake_finished
    }

    pub(crate) fn handshake_send(&mut self, io: &mut Io) {
        if self.handshake_finished {
            panic!("Handshake should be finished by now");
        }

        self.send_ping(io);
    }

    /// Returns whether a ping message should be sent
    pub fn connection_send(&mut self, io: &mut Io) -> bool {
        if self.ping_timer.ringing() {

            self.ping_timer.reset();

            self.send_ping(io);

            return true;
        }

        return false;
    }

    /// Get an outgoing ping payload
    fn send_ping(&mut self, io: &mut Io) {

        let mut writer = BitWriter::new();

        // write header
        StandardHeader::new(PacketType::Ping, 0, 0, 0).ser(&mut writer);

        // Record ping
        let ping_index = self.sent_pings.push_new();

        // write index
        ping_index.ser(&mut writer);

        //info!("sent Ping: {ping_index} to Server");

        // send packet
        if io.send_writer(&mut writer).is_err() {
            // TODO: pass this on and handle above
            warn!("Client Error: Cannot send ping packet to Server");
        }
    }

    /// Process an incoming pong payload
    pub fn process_pong(&mut self, reader: &mut BitReader) -> Result<(), SerdeErr> {
        let server_tick = Tick::de(reader)?;
        let ping_index = PingIndex::de(reader)?;

        //info!("received Ping: {ping_index} from Server");

        let Some(ping_instant) = self.sent_pings.remove(ping_index) else {
            warn!("Unknown pong received");
            return Ok(());
        };
        let rtt_millis = &ping_instant.elapsed().as_secs_f32() * 1000.0;
        self.process_new_rtt(rtt_millis);

        if !self.handshake_finished {
            self.handshake_pongs_received += 1;
            if self.handshake_pongs_received >= HANDSHAKE_PONGS_REQUIRED {
                self.handshake_finished = true;
            }
        }

        Ok(())
    }

    /// Recompute rtt/jitter estimations
    fn process_new_rtt(&mut self, rtt_millis: f32) {
        // TODO: return to proper standard deviation measure
        let new_jitter = ((rtt_millis - self.rtt) / 2.0).abs();

        self.jitter = (0.9 * self.jitter) + (0.1 * new_jitter);
        self.rtt = (0.9 * self.rtt) + (0.1 * rtt_millis);
    }

    pub(crate) fn read_server_tick(&self, reader: &mut BitReader) -> Tick {
        todo!()
    }
    pub(crate) fn recv_client_tick(&self) -> bool {
        todo!()
    }
    pub(crate) fn write_client_tick(&self, writer: &mut BitWriter) -> Tick {
        todo!()
    }
    pub(crate) fn client_sending_tick(&self) -> Tick {
        todo!()
    }
    pub(crate) fn client_receiving_tick(&self) -> Tick {
        todo!()
    }
    pub(crate) fn server_receivable_tick(&self) -> Tick {
        todo!()
    }
    pub(crate) fn interpolation(&self) -> f32 {
        todo!()
    }
}
