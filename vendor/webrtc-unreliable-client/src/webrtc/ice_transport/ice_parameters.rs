/// ICEParameters includes the ICE username fragment
/// and password and other ICE-related parameters.
#[derive(Default, Debug, Clone, PartialEq)]
pub(crate) struct RTCIceParameters {
    pub(crate) username_fragment: String,
    pub(crate) password: String,
}
