use std::fmt;
use std::sync::Arc;

use crate::webrtc::ice::candidate::candidate_base::CandidateBaseConfig;
use crate::webrtc::ice::candidate::candidate_host::CandidateHostConfig;
use crate::webrtc::ice::candidate::Candidate;

use crate::webrtc::error::{Error, Result};
use crate::webrtc::ice_transport::ice_candidate_type::RTCIceCandidateType;
use crate::webrtc::ice_transport::ice_protocol::RTCIceProtocol;

/// ICECandidate represents a ice candidate
#[derive(Default, Debug, Clone, PartialEq)]
pub(crate) struct RTCIceCandidate {
    pub(crate) stats_id: String,
    pub(crate) foundation: String,
    pub(crate) priority: u32,
    pub(crate) address: String,
    pub(crate) protocol: RTCIceProtocol,
    pub(crate) port: u16,
    pub(crate) typ: RTCIceCandidateType,
    pub(crate) component: u16,
    pub(crate) related_address: String,
    pub(crate) related_port: u16,
}

/// Conversion for ice_candidates
pub(crate) fn rtc_ice_candidates_from_ice_candidates(
    ice_candidates: &[Arc<dyn Candidate + Send + Sync>],
) -> Vec<RTCIceCandidate> {
    ice_candidates.iter().map(|c| c.into()).collect()
}

impl From<&Arc<dyn Candidate + Send + Sync>> for RTCIceCandidate {
    fn from(c: &Arc<dyn Candidate + Send + Sync>) -> Self {
        let typ: RTCIceCandidateType = c.candidate_type().into();
        let protocol = RTCIceProtocol::from(c.network_type().network_short().as_str());
        let (related_address, related_port) = if let Some(ra) = c.related_address() {
            (ra.address, ra.port)
        } else {
            (String::new(), 0)
        };

        RTCIceCandidate {
            stats_id: c.id(),
            foundation: c.foundation(),
            priority: c.priority(),
            address: c.address(),
            protocol,
            port: c.port(),
            component: c.component(),
            typ,
            related_address,
            related_port,
        }
    }
}

impl RTCIceCandidate {
    pub(crate) async fn to_ice(&self) -> Result<impl Candidate> {
        let candidate_id = self.stats_id.clone();
        let c = match self.typ {
            RTCIceCandidateType::Host => {
                let config = CandidateHostConfig {
                    base_config: CandidateBaseConfig {
                        candidate_id,
                        network: self.protocol.to_string(),
                        address: self.address.clone(),
                        port: self.port,
                        component: self.component,
                        //tcp_type: ice.NewTCPType(c.TCPType),
                        foundation: self.foundation.clone(),
                        priority: self.priority,
                        ..Default::default()
                    },
                };
                config.new_candidate_host().await?
            }
            _ => return Err(Error::ErrICECandidateTypeUnknown),
        };

        Ok(c)
    }
}

impl fmt::Display for RTCIceCandidate {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} {} {}:{}{}",
            self.protocol, self.typ, self.address, self.port, self.related_address,
        )
    }
}
