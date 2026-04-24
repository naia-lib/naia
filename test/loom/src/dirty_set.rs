// Loom concurrency model-checking for Win-3: dirty-receiver candidate set.
//
// Win-3 adds a push-based dirty set per user connection. Mutation callbacks
// write into the set while the send thread drains it each tick.
// Loom exhaustively explores all valid thread interleavings to detect
// data races and lost-update bugs that ordinary tests miss.
//
// Run with:  RUSTFLAGS="--cfg loom" cargo test -p naia-loom

#[cfg(loom)]
mod tests {
    use loom::sync::{Arc, Mutex};
    use loom::thread;
    use std::collections::HashSet;

    // Simulated dirty-component tracking: a set of component keys that have
    // been mutated this tick. The mutator thread writes; the send thread reads
    // and drains.
    #[test]
    fn dirty_set_concurrent_mutate_drain() {
        loom::model(|| {
            // Shared dirty set guarded by a Mutex — mirrors the DiffMask/dirty
            // set pattern in naia_shared::world::update::user_diff_handler.
            let dirty: Arc<Mutex<HashSet<u32>>> = Arc::new(Mutex::new(HashSet::new()));

            let dirty_mutator = Arc::clone(&dirty);
            let dirty_drainer = Arc::clone(&dirty);

            // Mutator thread: marks component key 42 as dirty.
            let mutator = thread::spawn(move || {
                dirty_mutator.lock().unwrap().insert(42);
            });

            // Drainer thread: drains the dirty set (simulates tick send phase).
            let drainer = thread::spawn(move || {
                let drained: HashSet<u32> = {
                    let mut guard = dirty_drainer.lock().unwrap();
                    std::mem::take(&mut *guard)
                };
                // Every key in the drained set must have been a valid dirty mark.
                for key in &drained {
                    assert!(*key == 42, "unexpected key {}", key);
                }
            });

            mutator.join().unwrap();
            drainer.join().unwrap();

            // After both threads complete, dirty set must not contain phantom entries.
            let remaining = dirty.lock().unwrap().len();
            // Either the drainer ran before the mutator (remaining == 1)
            // or after (remaining == 0). Both are valid — no phantom entries.
            assert!(remaining <= 1);
        });
    }

    // Ensures that concurrent inserts from two mutator threads don't lose entries.
    #[test]
    fn dirty_set_two_concurrent_mutators() {
        loom::model(|| {
            let dirty: Arc<Mutex<HashSet<u32>>> = Arc::new(Mutex::new(HashSet::new()));

            let d1 = Arc::clone(&dirty);
            let d2 = Arc::clone(&dirty);

            let t1 = thread::spawn(move || d1.lock().unwrap().insert(1));
            let t2 = thread::spawn(move || d2.lock().unwrap().insert(2));

            t1.join().unwrap();
            t2.join().unwrap();

            let guard = dirty.lock().unwrap();
            // Both inserts must have succeeded — no lost update.
            assert!(guard.contains(&1), "key 1 was lost");
            assert!(guard.contains(&2), "key 2 was lost");
        });
    }
}
