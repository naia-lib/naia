#[cfg(feature = "e2e_debug")]
use std::sync::atomic::AtomicUsize;

#[cfg(feature = "e2e_debug")]
pub static CLIENT_RX_SET_AUTH: AtomicUsize = AtomicUsize::new(0);
#[cfg(feature = "e2e_debug")]
pub static CLIENT_HANDLE_SET_AUTH: AtomicUsize = AtomicUsize::new(0);
#[cfg(feature = "e2e_debug")]
pub static CLIENT_EMIT_AUTH_GRANTED_EVENT: AtomicUsize = AtomicUsize::new(0);
#[cfg(feature = "e2e_debug")]
pub static CLIENT_WORLD_PKTS_RECV: AtomicUsize = AtomicUsize::new(0);
#[cfg(feature = "e2e_debug")]
pub static CLIENT_SAW_SET_AUTH_WIRE: AtomicUsize = AtomicUsize::new(0);
#[cfg(feature = "e2e_debug")]
pub static CLIENT_TO_EVENT_SET_AUTH_OK: AtomicUsize = AtomicUsize::new(0);
#[cfg(feature = "e2e_debug")]
pub static CLIENT_SAW_SPAWN: AtomicUsize = AtomicUsize::new(0);
#[cfg(feature = "e2e_debug")]
pub static CLIENT_ROUTED_REMOTE_SPAWN: AtomicUsize = AtomicUsize::new(0);
#[cfg(feature = "e2e_debug")]
pub static CLIENT_PROCESSED_SPAWN: AtomicUsize = AtomicUsize::new(0);
#[cfg(feature = "e2e_debug")]
pub static CLIENT_SCOPE_APPLIED_ADD_E2: AtomicUsize = AtomicUsize::new(0);
#[cfg(feature = "e2e_debug")]
pub static CLIENT_SCOPE_APPLIED_REMOVE_E1: AtomicUsize = AtomicUsize::new(0);
