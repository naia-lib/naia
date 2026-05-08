//! Parameter vocabulary for namako step bindings.
//!
//! **Purpose:** define the canonical, typed parameter shapes that step
//! phrases use, so the catalog stays unambiguous and grep-friendly as
//! it scales.
//!
//! ## Discipline rules
//!
//! 1. A new step phrase MUST use one of the parameter types declared
//!    here. If you find yourself wanting an ad-hoc shape, add it to
//!    `vocab.rs` first and reuse it.
//! 2. Parameter NAMES are part of the contract. `{client}` always means
//!    a registered client by name; `{entity}` always means a stored
//!    entity reference. Do not rename a parameter in one place and not
//!    others — that's the road back to today's "7 variants of 'client
//!    is connected'" mess.
//! 3. Built-in cucumber-rs parameters (`{int}`, `{float}`, `{word}`,
//!    `{string}`) are still available, but PREFER the typed wrappers
//!    here when the value carries domain meaning.
//!
//! ## Vocabulary
//!
//! | Param | Captured Rust type | Meaning |
//! |---|---|---|
//! | `{client}` | [`ClientName`] | Named client ("alice", "bob", "A", "B"). Resolves via `Scenario`'s registered clients. |
//! | `{entity}` | [`EntityRef`] | Symbolic entity label ("A", "B"). Resolves via `entity_label_to_key_storage`. |
//!
//! Built-in `{int}`/`{float}`/`{word}` continue to work for raw numerics
//! and values without domain meaning (protocol versions, replication configs, etc.).
//!
//! ## Why newtypes instead of `String` everywhere?
//!
//! Two reasons:
//! 1. The binding signature documents intent: `client: ClientName` is
//!    self-evidently a client name, vs `client: String` which could be
//!    anything.
//! 2. The cucumber-rs parser uses [`namako_engine::Parameter`] regex
//!    matching, so the parameter name in the phrase (`{client}`) must
//!    map to a Rust type that implements `Parameter`. Newtypes give us
//!    that type safely without depending on `String` collisions.

use std::fmt;
use std::str::FromStr;

use namako_engine::Parameter;

// ──────────────────────────────────────────────────────────────────────
// {client} — named client lookup.
// ──────────────────────────────────────────────────────────────────────

/// Named client (e.g. `"alice"`, `"A"`, `"client_a"`). Resolves to a
/// [`ClientKey`](crate::ClientKey) via `Scenario`'s registered name
/// map. Used in phrases like `"client {client} sees the entity"`.
///
/// Regex matches a single bare word (no whitespace) — by far the most
/// common pattern in legacy tests.
#[derive(Parameter, Clone, Debug, PartialEq, Eq, Hash)]
#[param(regex = r"[A-Za-z][A-Za-z0-9_]*", name = "client")]
pub struct ClientName(pub String);

impl FromStr for ClientName {
    type Err = std::convert::Infallible;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self(s.to_string()))
    }
}

impl fmt::Display for ClientName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl AsRef<str> for ClientName {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

// ──────────────────────────────────────────────────────────────────────
// {entity} — symbolic entity reference (BDD-store key).
// ──────────────────────────────────────────────────────────────────────

/// Symbolic entity reference. Resolves to a stored
/// [`naia_demo_world::Entity`] via the scenario's BDD store. Used in
/// phrases like `"the server mutates entity {entity}'s component"`.
///
/// Convention: short labels like `A`, `B`, `alice`. The regex accepts
/// both uppercase and lowercase so step phrases can use single-letter
/// labels like "A" or "B" that identify named test entities.
#[derive(Parameter, Clone, Debug, PartialEq, Eq, Hash)]
#[param(regex = r"[A-Za-z][A-Za-z0-9_]*", name = "entity")]
pub struct EntityRef(pub String);

impl FromStr for EntityRef {
    type Err = std::convert::Infallible;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self(s.to_string()))
    }
}

impl fmt::Display for EntityRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl AsRef<str> for EntityRef {
    fn as_ref(&self) -> &str {
        &self.0
    }
}


// ──────────────────────────────────────────────────────────────────────
// Tests
// ──────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn client_name_round_trip() {
        let n = ClientName::from_str("alice").unwrap();
        assert_eq!(n.to_string(), "alice");
        assert_eq!(n.as_ref(), "alice");
    }

    #[test]
    fn parameter_const_names_match_module_doc() {
        assert_eq!(ClientName::NAME, "client");
        assert_eq!(EntityRef::NAME, "entity");
    }
}
