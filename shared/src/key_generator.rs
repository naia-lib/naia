use std::{collections::VecDeque, fmt::Debug, marker::PhantomData, time::Duration};

use naia_socket_shared::Instant;

/// The integer width a [`KeyGenerator`] issues keys from.
///
/// This exists so one generator can serve both the narrow namespaces (message
/// indices, waitlist handles) and the wide ones (host entity ids) without
/// forking the recycling logic. It is deliberately tiny: a generator needs a
/// zero, a checked increment, and a name to put in the exhaustion panic.
pub trait KeyInt: Copy + Eq + Debug {
    /// The width's name, used only in the exhaustion panic message.
    const NAME: &'static str;
    /// The first key a fresh generator issues.
    const ZERO: Self;
    /// `self + 1`, or `None` if that would overflow the width.
    fn checked_increment(self) -> Option<Self>;
}

impl KeyInt for u16 {
    const NAME: &'static str = "u16";
    const ZERO: Self = 0;
    fn checked_increment(self) -> Option<Self> {
        self.checked_add(1)
    }
}

impl KeyInt for u32 {
    const NAME: &'static str = "u32";
    const ZERO: Self = 0;
    fn checked_increment(self) -> Option<Self> {
        self.checked_add(1)
    }
}

/// A store that manages a recycling pool of keys.
///
/// A freed key is quarantined for `recycle_timeout` before it may be reissued,
/// so a peer holding a stale reference cannot have it silently resolve to a
/// different object. That quarantine is what makes the *width* matter: the
/// ceiling is not "how many keys are live at once" but "how many keys are
/// issued per `recycle_timeout` window", because nothing returns to the pool
/// until its timer expires.
///
/// `I` defaults to `u16` so the narrow namespaces read exactly as before.
#[derive(Clone)]
pub struct KeyGenerator<K: From<I> + Into<I> + Copy, I: KeyInt = u16> {
    recycling_keys: VecDeque<(I, Instant)>,
    recycled_keys: VecDeque<I>,
    recycle_timeout: Duration,
    next_new_key: I,
    phantom: PhantomData<K>,
}

impl<K: From<I> + Into<I> + Copy, I: KeyInt> KeyGenerator<K, I> {
    /// Creates a `KeyGenerator` that holds recycled keys for at least `recycle_timeout` before reissuing them.
    pub fn new(recycle_timeout: Duration) -> Self {
        Self {
            recycle_timeout,
            recycling_keys: VecDeque::new(),
            recycled_keys: VecDeque::new(),
            next_new_key: I::ZERO,
            phantom: PhantomData,
        }
    }

    /// Get a new, unused key
    pub fn generate(&mut self) -> K {
        let now = Instant::now();

        // Check whether we can recycle any keys
        loop {
            let Some((_, instant)) = self.recycling_keys.front() else {
                break;
            };
            if instant.elapsed(&now) < self.recycle_timeout {
                break;
            }
            let (key, _) = self.recycling_keys.pop_front().unwrap();
            self.recycled_keys.push_back(key);
        }

        // Check whether we can return a recycled key
        if let Some(key) = self.recycled_keys.pop_front() {
            return K::from(key);
        }

        // Create a new key.
        //
        // Exhaustion PANICS rather than wrapping. Wrapping would hand out a key
        // that is still live, so two distinct objects would share an id and the
        // receiver would silently resolve one as the other — data corruption
        // that surfaces arbitrarily far from its cause. A panic here is loud,
        // immediate, and points at the real problem (the width is too narrow for
        // the issue rate).
        let output = self.next_new_key;
        self.next_new_key = self.next_new_key.checked_increment().unwrap_or_else(|| {
            panic!(
                "KeyGenerator exhausted: all {} keys are in use. Note this is a \
                 RATE limit, not a capacity limit — freed keys are quarantined \
                 for {:?} before reissue, so the ceiling is keys issued per that \
                 window, not keys live at once.",
                I::NAME,
                self.recycle_timeout,
            )
        });
        K::from(output)
    }

    /// Recycle a used key, freeing it up
    pub fn recycle_key(&mut self, key: &K) {
        let key_raw: I = Into::<I>::into(*key);
        self.recycling_keys.push_back((key_raw, Instant::now()));
    }

    /// Highest key ever issued + 1 — a lower bound for sizing dense side tables.
    pub fn capacity_hint(&self) -> I {
        self.next_new_key
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_are_issued_in_order_then_recycled_after_the_quarantine() {
        let mut g: KeyGenerator<u16> = KeyGenerator::new(Duration::from_millis(0));
        assert_eq!(g.generate(), 0);
        assert_eq!(g.generate(), 1);
        g.recycle_key(&0);
        // A zero quarantine means the freed key is immediately eligible again.
        assert_eq!(g.generate(), 0);
    }

    #[test]
    fn a_quarantined_key_is_not_reissued_while_its_timer_runs() {
        let mut g: KeyGenerator<u16> = KeyGenerator::new(Duration::from_secs(60));
        let first = g.generate();
        g.recycle_key(&first);
        // Still quarantined, so the generator must mint a fresh key instead of
        // handing `first` back out.
        assert_ne!(g.generate(), first);
    }

    #[test]
    fn the_u32_width_issues_keys_beyond_the_u16_ceiling() {
        // The point of the wider width: with everything quarantined, a u16
        // generator panics at 65_536 issues. Drive a u32 generator past that
        // boundary to prove the ceiling genuinely moved rather than merely
        // being relabelled.
        let mut g: KeyGenerator<u32, u32> = KeyGenerator::new(Duration::from_secs(60));
        for expected in 0..=65_536u32 {
            assert_eq!(g.generate(), expected);
        }
        assert_eq!(g.generate(), 65_537);
    }

    #[test]
    #[should_panic(expected = "KeyGenerator exhausted")]
    fn exhaustion_panics_rather_than_wrapping_into_live_keys() {
        // Anti-vacuity for the panic path: a wrapping generator would return 0
        // here (still live), silently aliasing two objects onto one id.
        let mut g: KeyGenerator<u16> = KeyGenerator::new(Duration::from_secs(60));
        for _ in 0..=u16::MAX {
            let _ = g.generate();
        }
    }
}
