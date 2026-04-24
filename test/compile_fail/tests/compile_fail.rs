/// Compile-fail harness for Naia derive macros (Phase 5).
///
/// Each fixture must fail to compile, verifying the immutable-component type-system enforcement:
///   - `immutable_property` — Property<T> inside #[replicate(immutable)] is a compile error
///   - `immutable_entity_property` — EntityProperty inside #[replicate(immutable)] is a compile error
#[test]
fn compile_fail_fixtures() {
    let t = trybuild::TestCases::new();
    t.compile_fail("fixtures/immutable_property.rs");
    t.compile_fail("fixtures/immutable_entity_property.rs");
}
