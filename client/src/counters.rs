#[cfg(feature = "e2e_debug")]
use std::sync::atomic::AtomicUsize;

#[cfg(feature = "e2e_debug")]
/// e2e_debug counter: client rx set auth.
pub static CLIENT_RX_SET_AUTH: AtomicUsize = AtomicUsize::new(0);
#[cfg(feature = "e2e_debug")]
/// e2e_debug counter: client handle set auth.
pub static CLIENT_HANDLE_SET_AUTH: AtomicUsize = AtomicUsize::new(0);
#[cfg(feature = "e2e_debug")]
/// e2e_debug counter: client emit auth granted event.
pub static CLIENT_EMIT_AUTH_GRANTED_EVENT: AtomicUsize = AtomicUsize::new(0);
#[cfg(feature = "e2e_debug")]
/// e2e_debug counter: client world pkts recv.
pub static CLIENT_WORLD_PKTS_RECV: AtomicUsize = AtomicUsize::new(0);
#[cfg(feature = "e2e_debug")]
/// e2e_debug counter: client to event set auth ok.
pub static CLIENT_TO_EVENT_SET_AUTH_OK: AtomicUsize = AtomicUsize::new(0);
#[cfg(feature = "e2e_debug")]
/// e2e_debug counter: client scope applied add e2.
pub static CLIENT_SCOPE_APPLIED_ADD_E2: AtomicUsize = AtomicUsize::new(0);
#[cfg(feature = "e2e_debug")]
/// e2e_debug counter: client scope applied remove e1.
pub static CLIENT_SCOPE_APPLIED_REMOVE_E1: AtomicUsize = AtomicUsize::new(0);

#[cfg(feature = "e2e_debug")]
pub use naia_shared::diagnostic_counters::{
    CLIENT_PROCESSED_SPAWN, CLIENT_ROUTED_REMOTE_SPAWN, CLIENT_SAW_SET_AUTH_WIRE, CLIENT_SAW_SPAWN,
};
