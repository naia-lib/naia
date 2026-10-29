use crate::webrtc::stun::attributes::*;
use crate::webrtc::stun::checks::*;
use crate::webrtc::stun::message::*;

/// Common helper for ICE-{CONTROLLED,CONTROLLING} and represents the so-called Tiebreaker number.
#[derive(Default, PartialEq, Debug, Copy, Clone)]
pub(crate) struct TieBreaker(pub(crate) u64);

pub(crate) const TIE_BREAKER_SIZE: usize = 8; // 64 bit

impl TieBreaker {
    /// Adds Tiebreaker value to m as t attribute.
    pub(crate) fn add_to_as(
        self,
        m: &mut Message,
        t: AttrType,
    ) -> Result<(), crate::webrtc::stun::Error> {
        let mut v = vec![0; TIE_BREAKER_SIZE];
        v.copy_from_slice(&self.0.to_be_bytes());
        m.add(t, &v);
        Ok(())
    }

    /// Decodes Tiebreaker value in message getting it as for t type.
    pub(crate) fn get_from_as(
        &mut self,
        m: &Message,
        t: AttrType,
    ) -> Result<(), crate::webrtc::stun::Error> {
        let v = m.get(t)?;
        check_size(t, v.len(), TIE_BREAKER_SIZE)?;
        self.0 = u64::from_be_bytes([v[0], v[1], v[2], v[3], v[4], v[5], v[6], v[7]]);
        Ok(())
    }
}
/// Represents ICE-CONTROLLED attribute.
#[derive(Default, PartialEq, Debug, Copy, Clone)]
pub(crate) struct AttrControlled(pub(crate) u64);

impl Setter for AttrControlled {
    /// Adds ICE-CONTROLLED to message.
    fn add_to(&self, m: &mut Message) -> Result<(), crate::webrtc::stun::Error> {
        TieBreaker(self.0).add_to_as(m, ATTR_ICE_CONTROLLED)
    }
}

impl Getter for AttrControlled {
    /// Decodes ICE-CONTROLLED from message.
    fn get_from(&mut self, m: &Message) -> Result<(), crate::webrtc::stun::Error> {
        let mut t = TieBreaker::default();
        t.get_from_as(m, ATTR_ICE_CONTROLLED)?;
        self.0 = t.0;
        Ok(())
    }
}

/// Represents ICE-CONTROLLING attribute.
#[derive(Default, PartialEq, Debug, Copy, Clone)]
pub(crate) struct AttrControlling(pub(crate) u64);

impl Setter for AttrControlling {
    // add_to adds ICE-CONTROLLING to message.
    fn add_to(&self, m: &mut Message) -> Result<(), crate::webrtc::stun::Error> {
        TieBreaker(self.0).add_to_as(m, ATTR_ICE_CONTROLLING)
    }
}

impl Getter for AttrControlling {
    // get_from decodes ICE-CONTROLLING from message.
    fn get_from(&mut self, m: &Message) -> Result<(), crate::webrtc::stun::Error> {
        let mut t = TieBreaker::default();
        t.get_from_as(m, ATTR_ICE_CONTROLLING)?;
        self.0 = t.0;
        Ok(())
    }
}
