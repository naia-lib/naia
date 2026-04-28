use std::hash::Hash;
#[cfg(debug_assertions)]
use std::sync::atomic::{AtomicU64, Ordering};

use crate::{RoomKey, UserKey};

/// Push-based mirror of the `(room, user, entity)` tuples produced by
/// `WorldServer::scope_checks()`. Replaces the per-tick
/// `O(rooms × users × entities)` rebuild with `O(churn)` updates whenever
/// rooms, users, or entities mutate. `as_slice()` is `O(1)` and allocates
/// nothing.
///
/// The cache stores the world entity `E` directly (not `GlobalEntity`), so
/// per-tick reads pay zero `HashMap` lookups. Each mutation hook resolves
/// `GlobalEntity → E` once via `entity_to_global_entity` / its inverse,
/// then mirrors the tuple here.
pub(crate) struct ScopeChecksCache<E: Copy + Eq + Hash + Send + Sync> {
    tuples: Vec<(RoomKey, UserKey, E)>,
    // Tuples added since the last call to mark_pending_handled(). Game code
    // that uses "add everything to scope on first sight" can poll only this
    // slice, which is empty on every tick after initial load.
    pending: Vec<(RoomKey, UserKey, E)>,
    #[cfg(debug_assertions)]
    read_counter: AtomicU64,
}

impl<E: Copy + Eq + Hash + Send + Sync> ScopeChecksCache<E> {
    pub fn new() -> Self {
        Self {
            tuples: Vec::new(),
            pending: Vec::new(),
            #[cfg(debug_assertions)]
            read_counter: AtomicU64::new(0),
        }
    }

    pub fn as_slice(&self) -> &[(RoomKey, UserKey, E)] {
        &self.tuples
    }

    /// Returns only tuples added since the last `mark_pending_handled()` call.
    /// After initial entity/user load, this is empty every tick — zero work.
    pub fn pending_slice(&self) -> &[(RoomKey, UserKey, E)] {
        &self.pending
    }

    /// Clears the pending queue. Call after processing `pending_slice()`.
    pub fn mark_pending_handled(&mut self) {
        self.pending.clear();
    }

    /// Returns true once every `period` reads. Used by debug-build assertions
    /// in `WorldServer::scope_checks()` to amortize the slow-path equivalence
    /// check (default period 1024 — same as the plan §3 step 3 assertion).
    #[cfg(debug_assertions)]
    pub fn should_assert_equivalence(&self, period: u64) -> bool {
        let n = self.read_counter.fetch_add(1, Ordering::Relaxed).wrapping_add(1);
        period > 0 && n.is_multiple_of(period)
    }

    pub fn on_user_added_to_room<I: IntoIterator<Item = E>>(
        &mut self,
        room_key: RoomKey,
        user_key: UserKey,
        entities_in_room: I,
    ) {
        for entity in entities_in_room {
            self.tuples.push((room_key, user_key, entity));
            self.pending.push((room_key, user_key, entity));
        }
    }

    pub fn on_user_removed_from_room(&mut self, room_key: RoomKey, user_key: UserKey) {
        self.tuples
            .retain(|&(rk, uk, _)| rk != room_key || uk != user_key);
        self.pending
            .retain(|&(rk, uk, _)| rk != room_key || uk != user_key);
    }

    pub fn on_entity_added_to_room<I: IntoIterator<Item = UserKey>>(
        &mut self,
        room_key: RoomKey,
        entity: E,
        users_in_room: I,
    ) {
        for user_key in users_in_room {
            self.tuples.push((room_key, user_key, entity));
            self.pending.push((room_key, user_key, entity));
        }
    }

    pub fn on_entity_removed_from_room(&mut self, room_key: RoomKey, entity: E) {
        self.tuples
            .retain(|&(rk, _, e)| rk != room_key || e != entity);
        self.pending
            .retain(|&(rk, _, e)| rk != room_key || e != entity);
    }

    /// Single-pass removal of every tuple referencing `entity` across all
    /// rooms — invoked by `WorldServer::despawn_entity_worldless` so the
    /// cache stays in sync without an O(rooms × entities) per-room walk.
    pub fn on_entity_despawned(&mut self, entity: E) {
        self.tuples.retain(|&(_, _, e)| e != entity);
        self.pending.retain(|&(_, _, e)| e != entity);
    }

    pub fn on_room_destroyed(&mut self, room_key: RoomKey) {
        self.tuples.retain(|&(rk, _, _)| rk != room_key);
        self.pending.retain(|&(rk, _, _)| rk != room_key);
    }
}

#[cfg(test)]
mod tests {
    //! Unit tests for the push-based cache state machine. End-to-end
    //! equivalence with the slow-path recompute is enforced by the
    //! debug-build assertion in `WorldServer::scope_checks()` (every 1024th
    //! read), so these tests focus on the cache's own invariants under each
    //! mutation hook.
    //!
    //! Test entity type `E = u32` stands in for the world entity — the cache
    //! is generic over `E: Copy + Eq + Hash + Send + Sync`, so any such type
    //! exercises the same code paths.
    use std::collections::HashSet;

    use naia_shared::BigMapKey;

    use super::ScopeChecksCache;
    use crate::{RoomKey, UserKey};

    fn rk(n: u64) -> RoomKey {
        RoomKey::from_u64(n)
    }

    fn uk(n: u64) -> UserKey {
        UserKey::from_u64(n)
    }

    fn snapshot(cache: &ScopeChecksCache<u32>) -> HashSet<(RoomKey, UserKey, u32)> {
        cache.as_slice().iter().copied().collect()
    }

    /// Recompute the expected (room, user, entity) set from a description of
    /// rooms-with-users-and-entities — mirrors the slow-path
    /// `WorldServer::scope_checks_recompute_slow` for these tests.
    fn expected(rooms: &[(RoomKey, &[UserKey], &[u32])]) -> HashSet<(RoomKey, UserKey, u32)> {
        let mut out = HashSet::new();
        for &(rk, users, entities) in rooms {
            for &u in users {
                for &e in entities {
                    out.insert((rk, u, e));
                }
            }
        }
        out
    }

    #[test]
    fn empty_room_yields_empty_scope_checks() {
        let cache = ScopeChecksCache::<u32>::new();
        assert!(cache.as_slice().is_empty());
    }

    #[test]
    fn add_user_to_room_appends_tuples_for_all_entities() {
        let mut cache = ScopeChecksCache::<u32>::new();
        cache.on_user_added_to_room(rk(0), uk(0), [10u32, 20, 30]);
        assert_eq!(
            snapshot(&cache),
            expected(&[(rk(0), &[uk(0)], &[10, 20, 30])])
        );
    }

    #[test]
    fn remove_user_from_room_removes_only_that_users_tuples() {
        let mut cache = ScopeChecksCache::<u32>::new();
        cache.on_user_added_to_room(rk(0), uk(0), [10u32, 20]);
        cache.on_user_added_to_room(rk(0), uk(1), [10u32, 20]);
        cache.on_user_removed_from_room(rk(0), uk(0));
        assert_eq!(
            snapshot(&cache),
            expected(&[(rk(0), &[uk(1)], &[10, 20])])
        );
    }

    #[test]
    fn add_entity_to_room_appends_tuple_for_each_user() {
        let mut cache = ScopeChecksCache::<u32>::new();
        let empty: [u32; 0] = [];
        cache.on_user_added_to_room(rk(0), uk(0), empty);
        cache.on_user_added_to_room(rk(0), uk(1), empty);
        cache.on_entity_added_to_room(rk(0), 99u32, [uk(0), uk(1)]);
        assert_eq!(
            snapshot(&cache),
            expected(&[(rk(0), &[uk(0), uk(1)], &[99])])
        );
    }

    #[test]
    fn remove_entity_from_room_removes_tuple_for_each_user() {
        let mut cache = ScopeChecksCache::<u32>::new();
        cache.on_user_added_to_room(rk(0), uk(0), [10u32, 20]);
        cache.on_user_added_to_room(rk(0), uk(1), [10u32, 20]);
        cache.on_entity_removed_from_room(rk(0), 10);
        assert_eq!(
            snapshot(&cache),
            expected(&[(rk(0), &[uk(0), uk(1)], &[20])])
        );
    }

    #[test]
    fn entity_despawn_drops_tuple_across_all_rooms() {
        let mut cache = ScopeChecksCache::<u32>::new();
        cache.on_user_added_to_room(rk(0), uk(0), [10u32, 20]);
        cache.on_user_added_to_room(rk(1), uk(1), [10u32, 30]);
        cache.on_entity_despawned(10);
        let mut combined = expected(&[(rk(0), &[uk(0)], &[20])]);
        combined.extend(expected(&[(rk(1), &[uk(1)], &[30])]));
        assert_eq!(snapshot(&cache), combined);
    }

    #[test]
    fn room_destroyed_drops_all_tuples_for_room() {
        let mut cache = ScopeChecksCache::<u32>::new();
        cache.on_user_added_to_room(rk(0), uk(0), [10u32, 20]);
        cache.on_user_added_to_room(rk(1), uk(1), [30u32]);
        cache.on_room_destroyed(rk(0));
        assert_eq!(snapshot(&cache), expected(&[(rk(1), &[uk(1)], &[30])]));
    }

    #[test]
    fn multiple_rooms_independent() {
        let mut cache = ScopeChecksCache::<u32>::new();
        cache.on_user_added_to_room(rk(0), uk(0), [10u32]);
        cache.on_user_added_to_room(rk(1), uk(0), [20u32]);
        // Removing the user from room 0 must not touch room-1 tuples.
        cache.on_user_removed_from_room(rk(0), uk(0));
        assert_eq!(snapshot(&cache), expected(&[(rk(1), &[uk(0)], &[20])]));
    }

    /// Drives the cache through 10K randomized add/remove operations and
    /// re-derives the expected set from a parallel ground-truth model.
    /// Mirrors the plan's "churn_test_maintains_equivalence_with_recompute".
    #[test]
    fn churn_test_maintains_equivalence_with_recompute() {
        use std::collections::HashMap;

        // Ground-truth: per-room (users-in-room, entities-in-room) sets.
        let mut truth: HashMap<RoomKey, (HashSet<UserKey>, HashSet<u32>)> = HashMap::new();
        let mut cache = ScopeChecksCache::<u32>::new();

        let mut rng_state: u64 = 0xdead_beef_cafe_f00d;
        let mut next = || {
            // xorshift64 — deterministic, reproducible across runs.
            rng_state ^= rng_state << 13;
            rng_state ^= rng_state >> 7;
            rng_state ^= rng_state << 17;
            rng_state
        };

        const ROOMS: u64 = 4;
        const USERS: u64 = 6;
        const ENTITIES: u32 = 16;

        for _ in 0..10_000 {
            let op = next() % 6;
            let r = rk(next() % ROOMS);
            let u = uk(next() % USERS);
            let e: u32 = (next() % ENTITIES as u64) as u32;
            let entry = truth.entry(r).or_default();
            match op {
                0 => {
                    if entry.0.insert(u) {
                        cache.on_user_added_to_room(r, u, entry.1.iter().copied());
                    }
                }
                1 => {
                    if entry.0.remove(&u) {
                        cache.on_user_removed_from_room(r, u);
                    }
                }
                2 => {
                    if entry.1.insert(e) {
                        cache.on_entity_added_to_room(r, e, entry.0.iter().copied());
                    }
                }
                3 => {
                    if entry.1.remove(&e) {
                        cache.on_entity_removed_from_room(r, e);
                    }
                }
                4 => {
                    // Despawn entity globally — drop from every room's truth.
                    for (_, (_, ents)) in truth.iter_mut() {
                        ents.remove(&e);
                    }
                    cache.on_entity_despawned(e);
                }
                _ => {
                    truth.remove(&r);
                    cache.on_room_destroyed(r);
                }
            }
        }

        let mut expected_set = HashSet::new();
        for (rk, (users, entities)) in &truth {
            for u in users {
                for e in entities {
                    expected_set.insert((*rk, *u, *e));
                }
            }
        }
        assert_eq!(snapshot(&cache), expected_set);
    }
}
