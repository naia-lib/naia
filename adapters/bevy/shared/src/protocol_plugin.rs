use crate::Protocol;

/// A reusable bundle of protocol registrations (channels, messages,
/// components) applied via [`Protocol::add_plugin`].
pub trait ProtocolPlugin {
    /// Registers this plugin's channels, messages, and components on `protocol`.
    fn build(&self, protocol: &mut Protocol);
}
