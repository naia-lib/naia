use crate::webrtc::dtls::cipher_suite::*;
use crate::webrtc::dtls::config::*;
use crate::webrtc::dtls::conn::*;
use crate::webrtc::dtls::content::*;
use crate::webrtc::dtls::error::*;
use crate::webrtc::dtls::signature_hash_algorithm::*;

use futures_util::{select, FutureExt};
use log::*;
use std::fmt;
use std::time::Duration;

//use std::io::BufWriter;

// [RFC6347 Section-4.2.4]
//                      +-----------+
//                +---> | PREPARING | <--------------------+
//                |     +-----------+                      |
//                |           |                            |
//                |           | Buffer next flight         |
//                |           |                            |
//                |          \|/                           |
//                |     +-----------+                      |
//                |     |  SENDING  |<------------------+  | Send
//                |     +-----------+                   |  | HelloRequest
//        Receive |           |                         |  |
//           next |           | Send flight             |  | or
//         flight |  +--------+                         |  |
//                |  |        | Set retransmit timer    |  | Receive
//                |  |       \|/                        |  | HelloRequest
//                |  |  +-----------+                   |  | Send
//                +--)--|  WAITING  |-------------------+  | ClientHello
//                |  |  +-----------+   Timer expires   |  |
//                |  |         |                        |  |
//                |  |         +------------------------+  |
//        Receive |  | Send           Read retransmit      |
//           last |  | last                                |
//         flight |  | flight                              |
//                |  |                                     |
//               \|/\|/                                    |
//            +-----------+                                |
//            | FINISHED  | -------------------------------+
//            +-----------+
//                 |  /|\
//                 |   |
//                 +---+
//              Read retransmit
//           Retransmit last flight

#[derive(Copy, Clone, PartialEq)]
pub(crate) enum HandshakeState {
    Preparing,
    Sending,
    Waiting,
    Finished,
}

impl fmt::Display for HandshakeState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            HandshakeState::Preparing => write!(f, "Preparing"),
            HandshakeState::Sending => write!(f, "Sending"),
            HandshakeState::Waiting => write!(f, "Waiting"),
            HandshakeState::Finished => write!(f, "Finished"),
        }
    }
}

pub(crate) struct HandshakeConfig {
    pub(crate) local_cipher_suites: Vec<CipherSuiteId>, // Available CipherSuites
    pub(crate) local_signature_schemes: Vec<SignatureHashAlgorithm>, // Available signature schemes
    pub(crate) extended_master_secret: ExtendedMasterSecretType, // Policy for the Extended Master Support extension
    pub(crate) server_name: String,
    pub(crate) retransmit_interval: Duration,
    pub(crate) initial_epoch: u16,
}

impl Default for HandshakeConfig {
    fn default() -> Self {
        HandshakeConfig {
            local_cipher_suites: vec![],
            local_signature_schemes: vec![],
            extended_master_secret: ExtendedMasterSecretType::Disable,
            server_name: String::new(),
            retransmit_interval: Duration::from_secs(0),
            initial_epoch: 0,
        }
    }
}

pub(crate) fn srv_cli_str(is_client: bool) -> String {
    if is_client {
        return "client".to_owned();
    }
    "server".to_owned()
}

impl DTLSConn {
    pub(crate) async fn handshake(&mut self, mut state: HandshakeState) -> Result<()> {
        loop {
            trace!(
                "[handshake:{}] {}: {}",
                srv_cli_str(self.state.is_client),
                self.current_flight,
                state
            );

            if state == HandshakeState::Finished && !self.is_handshake_completed_successfully() {
                self.set_handshake_completed_successfully();
                self.handshake_done_tx.take(); // drop it by take
                return Ok(());
            }

            state = match state {
                HandshakeState::Preparing => self.prepare().await?,
                HandshakeState::Sending => self.send().await?,
                HandshakeState::Waiting => self.wait().await?,
                HandshakeState::Finished => self.finish().await?,
            };
        }
    }

    async fn prepare(&mut self) -> Result<HandshakeState> {
        self.flights = None;

        // Prepare flights
        self.retransmit = self.current_flight.has_retransmit();

        let result = self
            .current_flight
            .generate(&mut self.state, &self.cache, &self.cfg)
            .await;

        match result {
            Err((a, mut err)) => {
                if let Some(a) = a {
                    let alert_err = self.notify(a.alert_level, a.alert_description).await;

                    if let Err(alert_err) = alert_err {
                        if err.is_some() {
                            err = Some(alert_err);
                        }
                    }
                }
                if let Some(err) = err {
                    return Err(err);
                }
            }
            Ok(pkts) => {
                /*if !pkts.is_empty() {
                    let mut s = vec![];
                    {
                        let mut writer = BufWriter::<&mut Vec<u8>>::new(s.as_mut());
                        pkts[0].record.content.marshal(&mut writer)?;
                    }
                    trace!(
                        "[handshake:{}] {}: {:?}",
                        srv_cli_str(self.state.is_client),
                        self.current_flight.to_string(),
                        s,
                    );
                }*/
                self.flights = Some(pkts)
            }
        };

        let epoch = self.cfg.initial_epoch;
        let mut next_epoch = epoch;
        if let Some(pkts) = &mut self.flights {
            for p in pkts {
                p.record.record_layer_header.epoch += epoch;
                if p.record.record_layer_header.epoch > next_epoch {
                    next_epoch = p.record.record_layer_header.epoch;
                }
                if let Content::Handshake(h) = &mut p.record.content {
                    h.handshake_header.message_sequence = self.state.handshake_send_sequence as u16;
                    self.state.handshake_send_sequence += 1;
                }
            }
        }
        if epoch != next_epoch {
            trace!(
                "[handshake:{}] -> changeCipherSpec (epoch: {})",
                srv_cli_str(self.state.is_client),
                next_epoch
            );
            self.set_local_epoch(next_epoch);
        }

        Ok(HandshakeState::Sending)
    }
    async fn send(&mut self) -> Result<HandshakeState> {
        // Send flights
        if let Some(pkts) = self.flights.clone() {
            self.write_packets(pkts).await?;
        }

        if self.current_flight.is_last_send_flight() {
            Ok(HandshakeState::Finished)
        } else {
            Ok(HandshakeState::Waiting)
        }
    }
    async fn wait(&mut self) -> Result<HandshakeState> {
        loop {
            select! {
                 done = self.handshake_rx.recv().fuse() =>{
                    if done.is_err() {
                        trace!("[handshake:{}] {} handshake_tx is dropped", srv_cli_str(self.state.is_client), self.current_flight);
                        return Err(Error::ErrAlertFatalOrClose);
                    }

                    //trace!("[handshake:{}] {} received handshake_rx", srv_cli_str(self.state.is_client), self.current_flight.to_string());
                    let result = self.current_flight.parse(&mut self.handle_queue_tx, &mut self.state, &self.cache, &self.cfg).await;
                    drop(done);
                    match result {
                        Err((alert, mut err)) => {
                            trace!("[handshake:{}] {} result alert:{:?}, err:{:?}",
                                    srv_cli_str(self.state.is_client),
                                    self.current_flight,
                                    alert,
                                    err);

                            if let Some(alert) = alert {
                                let alert_err = self.notify(alert.alert_level, alert.alert_description).await;

                                if let Err(alert_err) = alert_err {
                                    if err.is_some() {
                                        err = Some(alert_err);
                                    }
                                }
                            }
                            if let Some(err) = err {
                                return Err(err);
                            }
                        }
                        Ok(next_flight) => {
                            trace!("[handshake:{}] {} -> {}", srv_cli_str(self.state.is_client), self.current_flight, next_flight);
                            if next_flight.is_last_recv_flight() && self.current_flight.to_string() == next_flight.to_string() {
                                return Ok(HandshakeState::Finished);
                            }
                            self.current_flight = next_flight;
                            return Ok(HandshakeState::Preparing);
                        }
                    };
                }

                _ = smol::Timer::after(self.cfg.retransmit_interval).fuse() =>{
                    trace!("[handshake:{}] {} retransmit_timer", srv_cli_str(self.state.is_client), self.current_flight);

                    if !self.retransmit {
                        return Ok(HandshakeState::Waiting);
                    }
                    return Ok(HandshakeState::Sending);
                }

                /*_ = self.done_rx.recv() => {
                    return Err(Error::new("done_rx recv".to_owned()));
                }*/
            }
        }
    }
    async fn finish(&mut self) -> Result<HandshakeState> {
        select! {
            done = self.handshake_rx.recv().fuse() =>{
                if done.is_err() {
                    trace!("[handshake:{}] {} handshake_tx is dropped", srv_cli_str(self.state.is_client), self.current_flight);
                    return Err(Error::ErrAlertFatalOrClose);
                }
                let result = self.current_flight.parse(&mut self.handle_queue_tx, &mut self.state, &self.cache, &self.cfg).await;
                drop(done);
                match result {
                    Err((alert, mut err)) => {
                        if let Some(alert) = alert {
                            let alert_err = self.notify(alert.alert_level, alert.alert_description).await;
                            if let Err(alert_err) = alert_err {
                                if err.is_some() {
                                    err = Some(alert_err);
                                }
                            }
                        }
                        if let Some(err) = err {
                            return Err(err);
                        }
                    }
                    Ok(_) => {
                        smol::Timer::after(self.cfg.retransmit_interval).await;
                        // Retransmit last flight
                        return Ok(HandshakeState::Sending);
                    }
                };
            }

            /*_ = self.done_rx.recv() => {
                return Err(Error::new("done_rx recv".to_owned()));
            }*/
        }

        Ok(HandshakeState::Finished)
    }
}
