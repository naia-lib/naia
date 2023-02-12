use crate::Protocol;

pub trait ProtocolPlugin {
    fn build(&self, protocol: &mut Protocol);
}