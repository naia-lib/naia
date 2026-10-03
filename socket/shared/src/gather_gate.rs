//! Decides when the wasm client POSTs its WebRTC session offer while ICE
//! gathering is still in flight.
//!
//! Roger 41874 (Playwright `RTCPeerConnection` instrumentation, dev stack):
//! gathering started at 5.288 s, a host mDNS candidate and an srflx UDP
//! candidate were both ready at 6.33 s, yet `iceGatheringState == complete`
//! did not fire until 46.396 s — a single slow STUN probe (icecandidateerror
//! 701, an ordinary STUN timeout on real networks) held the gate. The
//! client's 30 s connection deadline fired at ~37 s and reset the attempt,
//! so the UI stalled on Loading despite usable candidates. The gate must
//! therefore release the POST on a bounded wait with candidates in hand, not
//! on `complete`. A candidate-less early post stays forbidden: naia's
//! signaling is a single POST offer → answer with no trickle channel, so an
//! empty offer is unrecoverable; with zero candidates the 60 s gathering
//! backstop reports loudly instead.

/// Bound on ICE gathering before the WebRTC `offer` is sent with a partial
/// candidate set. Derived from Roger 41874 against the 30 s connection
/// deadline (`ConnectionConfig::disconnection_timeout`): usable `srflx`/host
/// candidates arrive ~1 s into gathering while a single dead STUN path can
/// withhold `complete` for ~40 s, so waiting past 10 s buys nothing and risks
/// the deadline reset. Gathering that completes first still posts the full
/// set immediately; gathering with zero candidates never early-posts (the
/// 60 s backstop in the wasm backend reports that loudly instead).
pub const ICE_GATHER_EARLY_POST_MS: i32 = 10_000;

/// Returns whether the wasm client should POST its session offer now.
/// Posts once gathering completes (full candidate set), or once
/// [`ICE_GATHER_EARLY_POST_MS`] has passed with at least one srflx/host
/// candidate in hand — naia's signaling is a single POST offer → answer with
/// no trickle channel, so the partial set is final and a candidate-less
/// offer would be unrecoverable.
#[must_use]
pub fn should_post_session_offer(
    gathering_complete: bool,
    candidate_count: u32,
    elapsed_ms: f64,
) -> bool {
    gathering_complete || (candidate_count > 0 && elapsed_ms >= f64::from(ICE_GATHER_EARLY_POST_MS))
}

#[cfg(test)]
mod tests {
    use super::{should_post_session_offer, ICE_GATHER_EARLY_POST_MS};

    /// Roger 41874 timeline: 2 candidates ready ~1 s in, `complete` stuck
    /// behind a dead STUN path until ~40 s, 30 s connection deadline in
    /// between. The POST must go out on the bounded wait, not on `complete`.
    #[test]
    fn early_post_with_candidates_before_complete() {
        // Gathering incomplete, host mDNS + srflx candidates in hand.
        assert!(!should_post_session_offer(false, 2, 1_000.0));
        assert!(!should_post_session_offer(false, 2, 9_999.0));
        // Bounded wait passes with candidates in hand: post without `complete`.
        assert!(should_post_session_offer(false, 2, 10_000.0));
        assert!(should_post_session_offer(false, 2, 40_000.0));
        assert_eq!(10_000, ICE_GATHER_EARLY_POST_MS);
    }

    /// Gathering that completes fast keeps the old behavior: post immediately
    /// with the full candidate set, no waiting.
    #[test]
    fn complete_posts_immediately_without_wait() {
        assert!(should_post_session_offer(true, 3, 500.0));
        assert!(should_post_session_offer(true, 0, 0.0));
    }

    /// A candidate-less early post is unrecoverable (no trickle channel), so
    /// the gate never releases it: hold for `complete` and let the 60 s
    /// gathering backstop report loudly instead.
    #[test]
    fn no_candidates_never_early_posts() {
        assert!(!should_post_session_offer(false, 0, 10_000.0));
        assert!(!should_post_session_offer(false, 0, 59_999.0));
    }
}
