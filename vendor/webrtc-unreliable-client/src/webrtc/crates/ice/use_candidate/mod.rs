use crate::webrtc::stun::attributes::ATTR_USE_CANDIDATE;
use crate::webrtc::stun::message::*;

/// Represents USE-CANDIDATE attribute.
#[derive(Default)]
pub(crate) struct UseCandidateAttr;

impl Setter for UseCandidateAttr {
    /// Adds USE-CANDIDATE attribute to message.
    fn add_to(&self, m: &mut Message) -> Result<(), crate::webrtc::stun::Error> {
        m.add(ATTR_USE_CANDIDATE, &[]);
        Ok(())
    }
}
