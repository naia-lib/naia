//! H3 (LOCAL_GUEST_PLAN): request-abandonment producer primitives.
//!
//! A `ConnectionRequestNonce` identifies one request/response exchange on a
//! live connection. Nonces are allocated by [`NonceAllocator`], never repeat
//! while the connection lives, and exhaust checked: running out is an error
//! the caller handles by retiring the connection, never by aliasing (no
//! wrap-around). `TransportTerminal::Abandoned` is the reliable,
//! payload-free terminal a peer sends when it abandons a pending response.

use std::fmt::{self, Debug, Display, Formatter};

use naia_serde::SerdeInternal;

/// Identifies one request/response exchange on a live connection.
///
/// Constructible only by [`NonceAllocator`] (or from the wire, once the H3
/// envelope cutover lands): application code can name a nonce it was given,
/// never mint one. `Copy` so routing tables can key on it freely; equality
/// is the raw value. `SerdeInternal` so the `RequestOrResponse` envelope
/// can carry it on the wire (H3 cutover, codec grammar 2).
#[derive(Clone, Copy, Eq, Hash, PartialEq, SerdeInternal)]
pub struct ConnectionRequestNonce {
    value: u64,
}

impl ConnectionRequestNonce {
    /// Reads the raw nonce value, for wire encoding and diagnostics.
    pub fn value(self) -> u64 {
        self.value
    }

    /// Rebuilds a nonce received on the wire. Only the transport decoder
    /// calls this: it names a peer-allocated nonce, it never allocates.
    pub fn from_wire(value: u64) -> Self {
        Self { value }
    }
}

impl Debug for ConnectionRequestNonce {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_tuple("ConnectionRequestNonce")
            .field(&self.value)
            .finish()
    }
}

/// The connection's nonce supply is spent: no further request may start on
/// it. The owner retires the connection normally; a fresh connection starts
/// a fresh supply. Never resolved by reusing a live nonce.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NonceExhaustion;

impl Display for NonceExhaustion {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.write_str("connection request nonce supply exhausted; retire the connection")
    }
}

impl std::error::Error for NonceExhaustion {}

/// Issues [`ConnectionRequestNonce`]s for one live connection.
///
/// Checked: [`next`](NonceAllocator::next) returns [`NonceExhaustion`]
/// instead of wrapping past `u64::MAX`, and stays exhausted. One allocator
/// per connection; a new connection starts a new allocator, so supplies
/// never span connections.
#[derive(Debug)]
pub struct NonceAllocator {
    next_value: u64,
    exhausted: bool,
}

impl NonceAllocator {
    /// Starts the supply at zero for a new connection.
    pub fn new() -> Self {
        Self {
            next_value: 0,
            exhausted: false,
        }
    }

    /// Starts the supply at `next_value`. Test and recovery hook: names
    /// where a fresh supply begins, never resumes a live one.
    pub fn with_next(next_value: u64) -> Self {
        Self {
            next_value,
            exhausted: false,
        }
    }

    /// Issues the next nonce, or [`NonceExhaustion`] when the supply is
    /// spent. Checked increment: `u64::MAX` is issued once, then the
    /// allocator is exhausted rather than wrapping to zero and aliasing a
    /// live nonce.
    ///
    /// Named `next`, not `next_nonce`: it is public API called from the
    /// client and server request paths, and it returns `Result`, so it
    /// cannot implement `Iterator`.
    #[allow(clippy::should_implement_trait)]
    pub fn next(&mut self) -> Result<ConnectionRequestNonce, NonceExhaustion> {
        if self.exhausted {
            return Err(NonceExhaustion);
        }
        let value = self.next_value;
        match self.next_value.checked_add(1) {
            Some(next_value) => self.next_value = next_value,
            None => self.exhausted = true,
        }
        Ok(ConnectionRequestNonce { value })
    }
}

impl Default for NonceAllocator {
    fn default() -> Self {
        Self::new()
    }
}

/// Reliable, payload-free terminal closing an abandoned exchange.
///
/// Carries only the [`ConnectionRequestNonce`] it terminates: never a typed
/// application payload, never a refusal or response. Receiving it (or a
/// late response) removes the remaining transport mapping, and a disposed
/// key cannot be recreated by a duplicate packet.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TransportTerminal {
    /// The peer abandoned the pending response for `nonce`.
    Abandoned {
        /// The nonce of the abandoned exchange.
        nonce: ConnectionRequestNonce,
    },
}

impl TransportTerminal {
    /// The nonce this terminal closes.
    pub fn nonce(self) -> ConnectionRequestNonce {
        match self {
            Self::Abandoned { nonce } => nonce,
        }
    }
}

/// Outcome of cancelling a locally-pending request.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CancelDisposition {
    /// The routing entry was removed and the transport request marked
    /// abandoned for `nonce`.
    Cancelled {
        /// The nonce of the cancelled exchange.
        nonce: ConnectionRequestNonce,
    },
    /// No routing entry exists for the key: already completed, already
    /// disposed, or never allocated.
    UnknownKey,
}

/// Outcome of abandoning a locally-pending response.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AbandonDisposition {
    /// Pending response routing was removed and the reliable terminal for
    /// `nonce` queued; late typed completions for the key cannot send.
    Abandoned {
        /// The nonce of the abandoned exchange.
        nonce: ConnectionRequestNonce,
    },
    /// No routing entry exists for the key: already answered, already
    /// abandoned, or never allocated.
    UnknownKey,
}

/// What a pending request holds when polled.
#[derive(Clone, Debug, PartialEq)]
pub enum RequestPoll<S> {
    /// No terminal state yet: neither response nor abandonment arrived.
    Pending,
    /// The typed response arrived.
    Response(S),
    /// The peer abandoned the exchange; no response will arrive.
    Abandoned,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allocator_issues_sequential_nonces_from_zero() {
        let mut allocator = NonceAllocator::new();
        let first = allocator.next().expect("fresh allocator has capacity");
        let second = allocator.next().expect("fresh allocator has capacity");
        assert_eq!(first.value(), 0);
        assert_eq!(second.value(), 1);
    }

    #[test]
    fn allocated_nonces_never_repeat() {
        let mut allocator = NonceAllocator::new();
        let mut seen = std::collections::HashSet::new();
        for _ in 0..10_000 {
            let nonce = allocator.next().expect("capacity remains");
            assert!(seen.insert(nonce), "nonce {:?} allocated twice", nonce);
        }
    }

    #[test]
    fn exhaustion_is_checked_not_wrapping() {
        let mut allocator = NonceAllocator::with_next(u64::MAX);
        let last = allocator.next().expect("u64::MAX is still allocatable");
        assert_eq!(last.value(), u64::MAX);
        assert_eq!(allocator.next(), Err(NonceExhaustion));
    }

    #[test]
    fn exhaustion_is_sticky() {
        let mut allocator = NonceAllocator::with_next(u64::MAX);
        let _ = allocator.next();
        assert_eq!(allocator.next(), Err(NonceExhaustion));
        assert_eq!(allocator.next(), Err(NonceExhaustion));
    }

    #[test]
    fn abandoned_terminal_carries_its_nonce() {
        let mut allocator = NonceAllocator::new();
        let nonce = allocator.next().expect("capacity remains");
        let terminal = TransportTerminal::Abandoned { nonce };
        assert_eq!(terminal.nonce(), nonce);
    }
}
