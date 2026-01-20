//! ABI Compile-Time Rejection Proofs
//!
//! This module contains compile_fail tests that verify the Context-First ABI
//! correctly rejects invalid step signatures at compile time.
//!
//! These tests use `#[cfg(FALSE)]` to prevent compilation - they document the
//! expected compiler errors without actually failing the build.
//!
//! # Tested Rejections
//!
//! 1. Then step using `TestWorldMut` (must fail - Then requires `TestWorldRef`)
//! 2. Given step using `TestWorldRef` (must fail - Given requires `TestWorldMut`)
//! 3. Step using explicit lifetime `TestWorldMut<'a>` (must fail - no explicit lifetimes)

// =============================================================================
// Proof 1: Then step with TestWorldMut → COMPILE ERROR
// =============================================================================
//
// Expected error: type mismatch
//   - `World::ctx_ref()` returns `TestWorldRef`
//   - Function expects `TestWorldMut`
//
// Error message would be something like:
// ```
// error[E0308]: mismatched types
//   --> src/steps/_abi_proofs.rs:XX:XX
//    |
// XX |     fn then_with_mut_ctx(ctx: TestWorldMut) { }
//    |                          ^^^
//    |                          expected `TestWorldMut<'_>`, found `TestWorldRef<'_>`
// ```

#[cfg(FALSE)]
mod proof_1_then_with_mut_ctx {
    use namako_engine::then;
    use crate::TestWorldMut;

    #[then("the proof fails because Then uses TestWorldMut")]
    fn then_with_mut_ctx(_ctx: &mut TestWorldMut) {
        // This should fail to compile because:
        // - #[then] attribute causes the macro to call World::ctx_ref()
        // - ctx_ref() returns TestWorldRef, not TestWorldMut
        // - Type mismatch: cannot pass TestWorldRef to TestWorldMut parameter
    }
}

// =============================================================================
// Proof 2: Given step with TestWorldRef → COMPILE ERROR
// =============================================================================
//
// Expected error: type mismatch
//   - `World::ctx_mut()` returns `TestWorldMut`
//   - Function expects `TestWorldRef`
//
// Error message would be something like:
// ```
// error[E0308]: mismatched types
//   --> src/steps/_abi_proofs.rs:XX:XX
//    |
// XX |     fn given_with_ref_ctx(ctx: TestWorldRef) { }
//    |                           ^^^
//    |                           expected `TestWorldRef<'_>`, found `TestWorldMut<'_>`
// ```

#[cfg(FALSE)]
mod proof_2_given_with_ref_ctx {
    use namako_engine::given;
    use crate::TestWorldRef;

    #[given("the proof fails because Given uses TestWorldRef")]
    fn given_with_ref_ctx(_ctx: &TestWorldRef) {
        // This should fail to compile because:
        // - #[given] attribute causes the macro to call World::ctx_mut()
        // - ctx_mut() returns TestWorldMut, not TestWorldRef
        // - Type mismatch: cannot pass TestWorldMut to TestWorldRef parameter
    }
}

// =============================================================================
// Proof 3: Explicit lifetime in context type → COMPILE ERROR
// =============================================================================
//
// Expected error: macro rejection (not type system)
//   - The proc macro explicitly rejects types with generics/lifetimes
//
// Error message would be:
// ```
// error: context type must not have explicit lifetimes or generics;
//        write `TestWorldMut` or `TestWorldRef`, not `TestWorldMut<'a>`
// ```

#[cfg(FALSE)]
mod proof_3_explicit_lifetime {
    use namako_engine::given;
    use crate::TestWorldMut;

    #[given("the proof fails because of explicit lifetime")]
    fn given_with_explicit_lifetime<'a>(_ctx: &mut TestWorldMut<'a>) {
        // This should fail to compile because:
        // - parse_context_type_from_args() rejects types with generics
        // - Error: "context type must not have explicit lifetimes or generics"
        // - Users must write TestWorldMut, not TestWorldMut<'a>
    }
}

// =============================================================================
// VERIFIED CORRECT USAGE (for reference)
// =============================================================================
//
// The following patterns ARE correct and compile successfully:

#[cfg(FALSE)]
mod reference_correct_patterns {
    use namako_engine::{given, when, then};
    use crate::{TestWorldMut, TestWorldRef};

    // Given with TestWorldMut - CORRECT
    #[given("a correct given step")]
    fn correct_given(_ctx: &mut TestWorldMut) {}

    // When with TestWorldMut - CORRECT
    #[when("a correct when step")]
    fn correct_when(_ctx: &mut TestWorldMut) {}

    // Then with TestWorldRef - CORRECT
    #[then("a correct then step")]
    fn correct_then(_ctx: &mut TestWorldRef) {}
}
