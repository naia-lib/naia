/// Compile-fail harness for Naia derive macros.
///
/// Phase 5 will add fixtures that must fail compilation:
///   - `immutable_property.rs` — Property<T> inside #[component(immutable)] is a compile error
///   - `immutable_entity_property.rs` — EntityProperty inside immutable component
///   - `delegated_immutable.rs` — Delegated config on immutable component
///
/// Until Phase 5 lands, this file asserts that the placeholder passes cleanly.
#[test]
fn compile_fail_fixtures() {
    let t = trybuild::TestCases::new();
    // Phase 5 will replace this with:
    //   t.compile_fail("fixtures/immutable_*.rs");
    //   t.compile_fail("fixtures/delegated_immutable.rs");
    t.pass("fixtures/placeholder.rs");
}
