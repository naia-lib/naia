use naia_serde::SerdeInternal;

#[derive(SerdeInternal, Debug, PartialEq, Eq, Clone, Copy)]
pub enum RejectReason {
    ProtocolMismatch,
    Auth,
}
