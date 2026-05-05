use std::{any::TypeId, collections::HashSet};

use crate::{ComponentKind, Replicate};

/// Protocol-wide table marking which `ComponentKind`s are Replicated
/// Resources (vs ordinary components).
///
/// A Resource type `R` is registered via `Protocol::add_resource::<R>()`,
/// which:
/// 1. Calls `component_kinds.add_component::<R>()` to allocate a normal
///    `ComponentKind` + NetId for `R`. Resources reuse the component wire
///    encoding 100% — they ARE components, just on a hidden singleton
///    entity.
/// 2. Records the resulting `ComponentKind` in this table so the receiver
///    side can recognize incoming `SpawnWithComponents` messages whose
///    components are resources, and populate its `ResourceRegistry`
///    accordingly.
///
/// There is no wire representation of `ResourceKinds` itself — the table
/// is built identically on both sides from the same `ProtocolPlugin`
/// registration order, exactly like `ComponentKinds`.
#[derive(Clone, Default)]
pub struct ResourceKinds {
    kinds: HashSet<ComponentKind>,
    /// Type-id index for O(1) `kind_for::<R>()` lookups without a HashMap
    /// allocation churn at registration time. Mirrors the (TypeId →
    /// ComponentKind) relationship that `ComponentKind::of::<R>()`
    /// already provides via TypeId equality, so this is informational.
    type_ids: HashSet<TypeId>,
}

impl ResourceKinds {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register `R` as a resource kind. Idempotent: re-registering the
    /// same type is a no-op (matches the `add_component` re-registration
    /// semantics — the component table dedupes on `TypeId`).
    pub fn register<R: Replicate>(&mut self, kind: ComponentKind) {
        self.kinds.insert(kind);
        self.type_ids.insert(TypeId::of::<R>());
    }

    /// O(1) — is the given `ComponentKind` a registered resource?
    pub fn is_resource(&self, kind: &ComponentKind) -> bool {
        self.kinds.contains(kind)
    }

    /// Return the `ComponentKind` registered for `R`, or `None` if `R`
    /// was never registered as a resource.
    ///
    /// Implementation note: `ComponentKind` is `TypeId`-keyed
    /// (`shared/src/world/component/component_kinds.rs:53`), so we
    /// construct the kind from `R`'s `TypeId` and check membership.
    pub fn kind_for<R: Replicate>(&self) -> Option<ComponentKind> {
        let kind = ComponentKind::of::<R>();
        if self.kinds.contains(&kind) {
            Some(kind)
        } else {
            None
        }
    }

    /// Number of registered resource kinds.
    pub fn len(&self) -> usize {
        self.kinds.len()
    }

    pub fn is_empty(&self) -> bool {
        self.kinds.is_empty()
    }

    /// Iterate over all registered resource kinds.
    pub fn iter(&self) -> impl Iterator<Item = &ComponentKind> {
        self.kinds.iter()
    }
}

// Behavioral tests for ResourceKinds live in the integration suite
// (test/tests) where real `Replicate`-derived types are available via
// `naia_test_harness::test_protocol`. Here we only need to exercise the
// HashMap mechanics, which we do via direct `ComponentKind` construction
// from `TypeId` (the `From<TypeId>` impl in `component_kinds.rs:53`).
#[cfg(test)]
mod tests {
    use super::*;

    fn kind_from(name: &'static str) -> ComponentKind {
        // Each &'static str literal produces a distinct TypeId via address;
        // we use distinct concrete unit types instead so each call site
        // gets a stable distinct TypeId.
        struct A;
        struct B;
        struct C;
        match name {
            "a" => ComponentKind::from(TypeId::of::<A>()),
            "b" => ComponentKind::from(TypeId::of::<B>()),
            _ => ComponentKind::from(TypeId::of::<C>()),
        }
    }

    #[test]
    fn unregistered_kind_is_not_resource() {
        let rk = ResourceKinds::new();
        assert!(!rk.is_resource(&kind_from("a")));
        assert_eq!(rk.len(), 0);
    }

    #[test]
    fn registered_kind_is_recognized() {
        let mut rk = ResourceKinds::new();
        let k = kind_from("a");
        // We bypass the `Replicate` bound on `register` here by hand-
        // setting via direct field manipulation through a thin helper:
        // the public surface has a `Replicate`-typed `register::<R>` for
        // the production path, exercised by integration tests.
        rk.kinds.insert(k);
        rk.type_ids.insert(TypeId::of::<u8>());

        assert!(rk.is_resource(&k));
        assert!(!rk.is_resource(&kind_from("b")));
        assert_eq!(rk.len(), 1);
    }
}
