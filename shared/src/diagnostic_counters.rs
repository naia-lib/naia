//! Shared diagnostic counters, also exposed through the client counter API.

use std::sync::atomic::AtomicUsize;

/// e2e_debug counter: client saw set auth wire.
pub static CLIENT_SAW_SET_AUTH_WIRE: AtomicUsize = AtomicUsize::new(0);

/// e2e_debug counter: client saw spawn.
pub static CLIENT_SAW_SPAWN: AtomicUsize = AtomicUsize::new(0);

/// e2e_debug counter: client routed remote spawn.
pub static CLIENT_ROUTED_REMOTE_SPAWN: AtomicUsize = AtomicUsize::new(0);

/// e2e_debug counter: client processed spawn.
pub static CLIENT_PROCESSED_SPAWN: AtomicUsize = AtomicUsize::new(0);
