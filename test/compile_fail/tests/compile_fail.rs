/// Compile-fail harness for Naia derive macros (Phase 5).
///
/// Verifies the immutable-component type-system enforcement:
///   - `immutable_entity_property` — EntityProperty inside #[replicate(immutable)] is a compile error
///     (entity relations require diff-tracking, which immutable components skip).
///
/// NOTE: `Property<T>` inside `#[replicate(immutable)]` is intentionally NOT a
/// compile error — immutable components carry their `Property` values once at
/// spawn/insert to seed each new observer (value-carrying seed-only
/// replication). Its former compile-fail fixture was removed when that
/// capability landed.
#[test]
fn compile_fail_fixtures() {
    let t = trybuild::TestCases::new();
    t.compile_fail("fixtures/immutable_entity_property.rs");
}
