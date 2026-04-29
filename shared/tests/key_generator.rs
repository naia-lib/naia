use std::time::Duration;
use naia_shared::KeyGenerator;

#[test]
fn key_generator_recycle_prevents_exhaustion() {
    let mut kg: KeyGenerator<u16> = KeyGenerator::new(Duration::from_millis(0));
    let k0 = kg.generate();
    kg.recycle_key(&k0);
    // After recycle with 0ms TTL, keys become available again.
    for _ in 0..1000 {
        let k = kg.generate();
        kg.recycle_key(&k);
    }
    // Should never panic.
}

#[test]
#[should_panic(expected = "KeyGenerator exhausted")]
fn key_generator_panics_before_exhaustion() {
    let mut kg: KeyGenerator<u16> = KeyGenerator::new(Duration::from_secs(60));
    for _ in 0..=65_535u32 {
        kg.generate();
    }
}
