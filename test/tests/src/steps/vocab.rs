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
//! | `{entity}` | [`EntityRef`] | Symbolic entity reference (BDD-store key). |
//! | `{component}` | [`ComponentName`] | Component-kind name (Position, Velocity, ...). |
//! | `{channel}` | [`ChannelName`] | Channel name (OrderedReliable, ...). |
//! | `{role}` | [`AuthRole`] | Authority role: `granted`, `denied`, `available`, `requested`, `releasing`. |
//! | `{room}` | [`RoomRef`] | Room reference (BDD-store key). |
//! | `{message}` | [`MessageName`] | Message-kind name. |
//!
//! Built-in `{int}`/`{float}` continue to work for raw numerics
//! (latency, tick counts, coordinates).
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
/// phrases like `"the {entity} replicates to client {client}"`.
///
/// Convention: short tags like `e`, `e1`, `entity_a`, `target`. Avoid
/// embedding type info in the name (`pos_entity` is worse than `e`).
// Phase A.3: migrate entity-label {word} bindings onto this type.
#[allow(dead_code)]
#[derive(Parameter, Clone, Debug, PartialEq, Eq, Hash)]
#[param(regex = r"[a-z][a-z0-9_]*", name = "entity")]
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
// {component} — component-kind name.
// ──────────────────────────────────────────────────────────────────────

/// Component-kind name (e.g. `"Position"`, `"Velocity"`,
/// `"ImmutableLabel"`). Maps to the test protocol's registered
/// component types.
// Phase A.3: migrate component-name {word} bindings onto this type.
#[allow(dead_code)]
#[derive(Parameter, Clone, Debug, PartialEq, Eq, Hash)]
#[param(regex = r"[A-Z][A-Za-z0-9]*", name = "component")]
pub struct ComponentName(pub String);

impl FromStr for ComponentName {
    type Err = std::convert::Infallible;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self(s.to_string()))
    }
}

impl fmt::Display for ComponentName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl AsRef<str> for ComponentName {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

// ──────────────────────────────────────────────────────────────────────
// {channel} — channel-kind name.
// ──────────────────────────────────────────────────────────────────────

/// Channel name (e.g. `"OrderedReliable"`, `"UnorderedUnreliable"`,
/// `"TickBuffered"`). Maps to the test protocol's registered channel
/// kinds.
// Phase A.3: migrate channel-name {word} bindings onto this type.
#[allow(dead_code)]
#[derive(Parameter, Clone, Debug, PartialEq, Eq, Hash)]
#[param(regex = r"[A-Z][A-Za-z0-9]*", name = "channel")]
pub struct ChannelName(pub String);

impl FromStr for ChannelName {
    type Err = std::convert::Infallible;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self(s.to_string()))
    }
}

impl fmt::Display for ChannelName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl AsRef<str> for ChannelName {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

// ──────────────────────────────────────────────────────────────────────
// {role} — authority role enum.
// ──────────────────────────────────────────────────────────────────────

/// Authority role for a delegated entity, as observable on the client
/// side. Maps to [`naia_shared::EntityAuthStatus`] for assertions.
///
/// Phrases: `"client {client} has {role} authority status for the entity"` →
/// `client A has granted authority status` / etc.
// Phase A.3: introduce consolidated authority-status assertions using this type.
#[allow(dead_code)]
#[derive(Parameter, Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[param(
    regex = r"granted|denied|available|requested|releasing",
    name = "role"
)]
pub enum AuthRole {
    Granted,
    Denied,
    Available,
    Requested,
    Releasing,
}

impl FromStr for AuthRole {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "granted" => Ok(Self::Granted),
            "denied" => Ok(Self::Denied),
            "available" => Ok(Self::Available),
            "requested" => Ok(Self::Requested),
            "releasing" => Ok(Self::Releasing),
            other => Err(format!("unrecognized {{role}} value: {other:?}")),
        }
    }
}

impl fmt::Display for AuthRole {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            Self::Granted => "granted",
            Self::Denied => "denied",
            Self::Available => "available",
            Self::Requested => "requested",
            Self::Releasing => "releasing",
        };
        f.write_str(s)
    }
}

// ──────────────────────────────────────────────────────────────────────
// {room} — room reference (BDD-store key).
// ──────────────────────────────────────────────────────────────────────

/// Symbolic room reference. Resolves to a stored
/// [`RoomKey`](crate::RoomKey) via the scenario's BDD store. Used in
/// phrases like `"client {client} joins room {room}"`.
// Phase A.3: introduce room-parameterized bindings onto this type.
#[allow(dead_code)]
#[derive(Parameter, Clone, Debug, PartialEq, Eq, Hash)]
#[param(regex = r"[a-z][a-z0-9_]*", name = "room")]
pub struct RoomRef(pub String);

impl FromStr for RoomRef {
    type Err = std::convert::Infallible;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self(s.to_string()))
    }
}

impl fmt::Display for RoomRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl AsRef<str> for RoomRef {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

// ──────────────────────────────────────────────────────────────────────
// {message} — message-kind name.
// ──────────────────────────────────────────────────────────────────────

/// Message-kind name (e.g. `"TestMessage"`, `"TestRequest"`). Maps to
/// the test protocol's registered message types.
// Phase A.3: migrate message-name {word} bindings onto this type.
#[allow(dead_code)]
#[derive(Parameter, Clone, Debug, PartialEq, Eq, Hash)]
#[param(regex = r"[A-Z][A-Za-z0-9]*", name = "message")]
pub struct MessageName(pub String);

impl FromStr for MessageName {
    type Err = std::convert::Infallible;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self(s.to_string()))
    }
}

impl fmt::Display for MessageName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl AsRef<str> for MessageName {
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
    fn auth_role_parse_all_variants() {
        for s in ["granted", "denied", "available", "requested", "releasing"] {
            let r = AuthRole::from_str(s).unwrap();
            assert_eq!(r.to_string(), s);
        }
    }

    #[test]
    fn auth_role_rejects_garbage() {
        assert!(AuthRole::from_str("nope").is_err());
    }

    #[test]
    fn parameter_const_names_match_module_doc() {
        assert_eq!(ClientName::NAME, "client");
        assert_eq!(EntityRef::NAME, "entity");
        assert_eq!(ComponentName::NAME, "component");
        assert_eq!(ChannelName::NAME, "channel");
        assert_eq!(AuthRole::NAME, "role");
        assert_eq!(RoomRef::NAME, "room");
        assert_eq!(MessageName::NAME, "message");
    }
}
