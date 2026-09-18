//! Deny-level regression guard for the 10882 derive-lint fix.
//!
//! The `Replicate` derive used to emit `source: source` (longhand) in
//! `new_complete` for NonReplicated fields, which trips
//! `-D clippy::redundant_field_names` in downstream gates (e.g. cyberlith).
//! The emitter now uses field-init shorthand; this file keeps the deny
//! pinned at the use site and exercises the derived constructor live.
//!
//! NOTE: a same-crate `deny` here does not reproduce the downstream fire
//! (clippy does not flag this macro's expansion in this workspace's
//! toolchain); the fail-before/pass-after proof lives in
//! `shared/derive_core/src/replicate.rs :: field_init_shorthand_tests`,
//! which asserts directly on the emitted tokens.

#![deny(clippy::redundant_field_names)]

use naia_shared::{Property, Replicate};

// A NonReplicated field (`source`) next to a replicated one (`value`):
// exactly the shape whose expansion carried the longhand init.
#[derive(Replicate)]
pub struct ShorthandProbe {
    pub value: Property<u8>,
    pub source: u32,
}

#[test]
fn derived_new_complete_constructs() {
    let probe = ShorthandProbe::new_complete(7, 42);
    assert_eq!(probe.source, 42);
    let _ = &probe.value;
}
