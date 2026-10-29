use super::*;
use crate::webrtc::ice::candidate::candidate_host::CandidateHostConfig;
use crate::webrtc::ice::error::*;
use crate::webrtc::ice::util::*;

use async_lock::Mutex;
use async_trait::async_trait;
use crc::{Crc, CRC_32_ISCSI};
use std::fmt;
use std::ops::Add;
use std::sync::atomic::{AtomicU16, AtomicU64, AtomicU8, Ordering};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

#[derive(Default)]
pub(crate) struct CandidateBaseConfig {
    pub(crate) candidate_id: String,
    pub(crate) network: String,
    pub(crate) address: String,
    pub(crate) port: u16,
    pub(crate) component: u16,
    pub(crate) priority: u32,
    pub(crate) foundation: String,
    pub(crate) conn: Option<Arc<dyn crate::webrtc::util::Conn + Send + Sync>>,
}

pub(crate) struct CandidateBase {
    pub(crate) id: String,
    pub(crate) network_type: AtomicU8,
    pub(crate) candidate_type: CandidateType,

    pub(crate) component: AtomicU16,
    pub(crate) address: String,
    pub(crate) port: u16,
    pub(crate) related_address: Option<CandidateRelatedAddress>,

    pub(crate) resolved_addr: Mutex<SocketAddr>,

    pub(crate) last_sent: AtomicU64,
    pub(crate) last_received: AtomicU64,

    pub(crate) conn: Option<Arc<dyn crate::webrtc::util::Conn + Send + Sync>>,
    pub(crate) closed_ch: Arc<Mutex<Option<async_broadcast::Sender<()>>>>,

    pub(crate) foundation_override: String,
    pub(crate) priority_override: u32,

    //CandidateHost
    pub(crate) network: String,
}

impl Default for CandidateBase {
    fn default() -> Self {
        Self {
            id: String::new(),
            network_type: AtomicU8::new(0),
            candidate_type: CandidateType::default(),

            component: AtomicU16::new(0),
            address: String::new(),
            port: 0,
            related_address: None,

            resolved_addr: Mutex::new(SocketAddr::new(IpAddr::from([0, 0, 0, 0]), 0)),

            last_sent: AtomicU64::new(0),
            last_received: AtomicU64::new(0),

            conn: None,
            closed_ch: Arc::new(Mutex::new(None)),

            foundation_override: String::new(),
            priority_override: 0,
            network: String::new(),
        }
    }
}

// String makes the candidateBase printable
impl fmt::Display for CandidateBase {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(related_address) = self.related_address() {
            write!(
                f,
                "{} {} {}:{}{}",
                self.network_type(),
                self.candidate_type(),
                self.address(),
                self.port(),
                related_address,
            )
        } else {
            write!(
                f,
                "{} {} {}:{}",
                self.network_type(),
                self.candidate_type(),
                self.address(),
                self.port(),
            )
        }
    }
}

#[async_trait]
impl Candidate for CandidateBase {
    fn foundation(&self) -> String {
        if !self.foundation_override.is_empty() {
            return self.foundation_override.clone();
        }

        let mut buf = vec![];
        buf.extend_from_slice(self.candidate_type().to_string().as_bytes());
        buf.extend_from_slice(self.address.as_bytes());
        buf.extend_from_slice(self.network_type().to_string().as_bytes());

        let checksum = Crc::<u32>::new(&CRC_32_ISCSI).checksum(&buf);

        format!("{}", checksum)
    }

    /// Returns Candidate ID.
    fn id(&self) -> String {
        self.id.clone()
    }

    /// Returns candidate component.
    fn component(&self) -> u16 {
        self.component.load(Ordering::SeqCst)
    }

    fn set_component(&self, component: u16) {
        self.component.store(component, Ordering::SeqCst);
    }

    /// Returns a time indicating the last time this candidate was received.
    fn last_received(&self) -> SystemTime {
        UNIX_EPOCH.add(Duration::from_nanos(
            self.last_received.load(Ordering::SeqCst),
        ))
    }

    /// Returns a time indicating the last time this candidate was sent.
    fn last_sent(&self) -> SystemTime {
        UNIX_EPOCH.add(Duration::from_nanos(self.last_sent.load(Ordering::SeqCst)))
    }

    /// Returns candidate NetworkType.
    fn network_type(&self) -> NetworkType {
        NetworkType::from(self.network_type.load(Ordering::SeqCst))
    }

    /// Returns Candidate Address.
    fn address(&self) -> String {
        self.address.clone()
    }

    /// Returns Candidate Port.
    fn port(&self) -> u16 {
        self.port
    }

    /// Computes the priority for this ICE Candidate.
    fn priority(&self) -> u32 {
        if self.priority_override != 0 {
            return self.priority_override;
        }

        // The local preference MUST be an integer from 0 (lowest preference) to
        // 65535 (highest preference) inclusive.  When there is only a single IP
        // address, this value SHOULD be set to 65535.  If there are multiple
        // candidates for a particular component for a particular data stream
        // that have the same type, the local preference MUST be unique for each
        // one.
        (1 << 24) * u32::from(self.candidate_type().preference())
            + (1 << 8) * u32::from(self.local_preference())
            + (256 - u32::from(self.component()))
    }

    /// Returns `Option<CandidateRelatedAddress>`.
    fn related_address(&self) -> Option<CandidateRelatedAddress> {
        self.related_address.as_ref().cloned()
    }

    /// Returns candidate type.
    fn candidate_type(&self) -> CandidateType {
        self.candidate_type
    }

    /// Returns the string representation of the ICECandidate.
    fn marshal(&self) -> String {
        let mut val = format!(
            "{} {} {} {} {} {} typ {}",
            self.foundation(),
            self.component(),
            self.network_type().network_short(),
            self.priority(),
            self.address(),
            self.port(),
            self.candidate_type()
        );

        if let Some(related_address) = self.related_address() {
            val += format!(
                " raddr {} rport {}",
                related_address.address, related_address.port,
            )
            .as_str();
        }

        val
    }

    async fn addr(&self) -> SocketAddr {
        let resolved_addr = self.resolved_addr.lock().await;
        *resolved_addr
    }

    /// Stops the recvLoop.
    async fn close(&self) -> Result<()> {
        {
            let mut closed_ch = self.closed_ch.lock().await;
            if closed_ch.is_none() {
                return Err(Error::ErrClosed);
            }
            closed_ch.take();
        }

        if let Some(conn) = &self.conn {
            let _ = conn.close().await;
        }

        Ok(())
    }

    fn seen(&self, outbound: bool) {
        let d = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_else(|_| Duration::from_secs(0));

        if outbound {
            self.set_last_sent(d);
        } else {
            self.set_last_received(d);
        }
    }

    async fn write_to(&self, raw: &[u8], dst: &(dyn Candidate + Send + Sync)) -> Result<usize> {
        let n = if let Some(conn) = &self.conn {
            let addr = dst.addr().await;
            conn.send_to(raw, addr).await?
        } else {
            0
        };
        self.seen(true);
        Ok(n)
    }

    /// Used to compare two candidateBases.
    fn equal(&self, other: &dyn Candidate) -> bool {
        self.network_type() == other.network_type()
            && self.candidate_type() == other.candidate_type()
            && self.address() == other.address()
            && self.port() == other.port()
            && self.related_address() == other.related_address()
    }

    async fn set_ip(&self, ip: &IpAddr) -> Result<()> {
        let network_type = determine_network_type(&self.network, ip)?;

        self.network_type
            .store(network_type as u8, Ordering::SeqCst);

        let mut resolved_addr = self.resolved_addr.lock().await;
        *resolved_addr = create_addr(network_type, *ip, self.port);

        Ok(())
    }

    fn get_conn(&self) -> Option<&Arc<dyn crate::webrtc::util::Conn + Send + Sync>> {
        self.conn.as_ref()
    }

    fn get_closed_ch(&self) -> Arc<Mutex<Option<async_broadcast::Sender<()>>>> {
        self.closed_ch.clone()
    }
}

impl CandidateBase {
    pub(crate) fn set_last_received(&self, d: Duration) {
        #[allow(clippy::cast_possible_truncation)]
        self.last_received
            .store(d.as_nanos() as u64, Ordering::SeqCst);
    }

    pub(crate) fn set_last_sent(&self, d: Duration) {
        #[allow(clippy::cast_possible_truncation)]
        self.last_sent.store(d.as_nanos() as u64, Ordering::SeqCst);
    }

    /// Returns the local preference for this candidate.
    pub(crate) fn local_preference(&self) -> u16 {
        DEFAULT_LOCAL_PREFERENCE
    }
}

/// Creates a Candidate from its string representation.
pub(crate) async fn unmarshal_candidate(raw: &str) -> Result<impl Candidate> {
    let split: Vec<&str> = raw.split_whitespace().collect();
    if split.len() < 8 {
        return Err(Error::Other(format!(
            "{:?} ({})",
            Error::ErrAttributeTooShortIceCandidate,
            split.len()
        )));
    }

    // Foundation
    let foundation = split[0].to_owned();

    // Component
    let component: u16 = split[1].parse()?;

    // Network
    let network = split[2].to_owned();

    // Priority
    let priority: u32 = split[3].parse()?;

    // Address
    let address = split[4].to_owned();

    // Port
    let port: u16 = split[5].parse()?;

    let typ = split[7];

    if split.len() > 8 {
        let split2 = &split[8..];

        if split2[0] == "raddr" && split2.len() < 4 {
            return Err(Error::Other(format!(
                "{:?}: incorrect length",
                Error::ErrParseRelatedAddr
            )));
        }
    }

    match typ {
        "host" => {
            let config = CandidateHostConfig {
                base_config: CandidateBaseConfig {
                    network,
                    address,
                    port,
                    component,
                    priority,
                    foundation,
                    ..CandidateBaseConfig::default()
                },
            };
            config.new_candidate_host().await
        }
        _ => Err(Error::Other(format!(
            "{:?} ({})",
            Error::ErrUnknownCandidateType,
            typ
        ))),
    }
}
