Refactor plan: `interior_visibility` feature for `naia` / `naia_server` / `naia_client` / `naia_shared`

1) Goals and constraints
   - Introduce a workspace-wide Cargo feature `interior_visibility`.
   - When enabled, expose a testing-oriented API that allows mapping between:
       - LocalEntity (u16) IDs and ECS entities on both client and server.
   - New public surface (only when feature is ON):
       - LocalEntity type alias in `naia_shared::world::local`.
       - Client.local_entities() -> Vec<LocalEntity>
       - Server.local_entities(UserKey) -> Vec<LocalEntity>
       - Client.local_entity(LocalEntity) -> EntityRef
       - Client.local_entity_mut(LocalEntity) -> EntityMut
       - Server.local_entity(UserKey, LocalEntity) -> EntityRef
       - Server.local_entity_mut(UserKey, LocalEntity) -> EntityMut
   - All new logic (other than minimal wiring) must live in `interior_visibility.rs` files:
       - server/src/interior_visibility.rs
       - client/src/interior_visibility.rs
       - shared/src/world/local/interior_visibility.rs
   - When the feature is disabled, the crate API and behavior remain exactly as today.

2) Cargo feature wiring (workspace + crates)
   - In the root `Cargo.toml`:
       - Define a new feature `interior_visibility` that fans out to the three core crates:
         interior_visibility = [
           "naia-server/interior_visibility",
           "naia-client/interior_visibility",
           "naia-shared/interior_visibility",
         ]
       - Ensure `default-features` does not include `interior_visibility`.
   - In `server/Cargo.toml` (naia-server), `client/Cargo.toml` (naia-client), and `shared/Cargo.toml` (naia-shared):
       - Add a `[features]` entry:
         interior_visibility = []
       - Ensure any docs.rs configuration / feature metadata lists `interior_visibility` appropriately.
   - If the top-level `naia` crate has its own `[features]` section for server/client, wire `interior_visibility` similarly so a single feature flag enables the behavior across all three crates.

3) Shared crate changes: LocalEntity definition + internal mapping helpers
   - File layout:
       - Add `shared/src/world/local/interior_visibility.rs`.
       - In `shared/src/world/local/mod.rs`:
           - Add `#[cfg(feature = "interior_visibility")] pub mod interior_visibility;`
           - Add `#[cfg(feature = "interior_visibility")] pub use interior_visibility::LocalEntity;`
   - In `shared/src/world/local/interior_visibility.rs`:
       - Define the core type alias:
           - `pub type LocalEntity = u16;`
       - Add internal (non-public-to-users) helper APIs to bridge between the existing replication bookkeeping and LocalEntity:
           - Read access to the LocalEntity collections that already exist for:
               - Server-side views (per-User LocalEntity → HostEntity/RemoteEntity mapping).
               - Client-side views (LocalEntity → Remote/Host entity mapping).
           - Conversion helpers:
               - LocalEntity → Global entity identifier used in shared world/replication.
               - Global entity identifier → LocalEntity (per user / per client).
       - These helpers should be `pub(crate)` and structured so that:
           - `naia_server::interior_visibility` can ask “for this UserKey, what LocalEntity IDs exist and what global entities do they refer to?”
           - `naia_client::interior_visibility` can ask “for this client, what LocalEntity IDs exist and what global entities do they refer to?”
       - Keep this module purely as a thin view on top of existing HostEntity/RemoteEntity/WorldManager machinery; do not alter the network protocol, serialization, or core replication logic.

4) Server-side API surface: exposing local-entity lookups on Server
   - File layout:
       - Add `server/src/interior_visibility.rs`.
       - In `server/src/lib.rs`:
           - Add `#[cfg(feature = "interior_visibility")] mod interior_visibility;`
           - Add `#[cfg(feature = "interior_visibility")] pub use interior_visibility::*;`
           - Optionally (for ergonomics): `#[cfg(feature = "interior_visibility")] pub use naia_shared::world::local::LocalEntity;`
   - In `server/src/interior_visibility.rs`:
       - `use` the types needed:
           - `use naia_shared::world::local::LocalEntity;`
           - `use crate::{Server, UserKey, EntityRef, EntityMut};` (or whatever exact paths/types exist).
           - Internal managers / world structures that know about LocalEntity mappings.
       - Add an `impl<E>` block for `Server<E>` (matching existing generics / bounds):
           - `pub fn local_entities(&self, user_key: &UserKey) -> Vec<LocalEntity>`
               - Returns the set of LocalEntity IDs that currently exist for that user (i.e., all entities replicated to that user).
               - The ordering doesn’t matter; document that explicitly.
           - `pub fn local_entity(&self, user_key: &UserKey, local: LocalEntity) -> EntityRef<'_>`
               - Resolves a LocalEntity for a given user into an `EntityRef` pointing at the underlying ECS entity in the server’s world.
           - `pub fn local_entity_mut(&mut self, user_key: &UserKey, local: LocalEntity) -> EntityMut<'_>`
               - Same as above, but returns a mutable reference wrapper.
       - Each of these methods should:
           - Delegate to the shared crate’s `LocalEntity` mapping helpers established in step 3.
           - Reuse existing internal accessors for going from global entity IDs to `EntityRef` / `EntityMut`, so the borrowing model and safety guarantees remain consistent with the rest of the Server API.
       - Keep all of this behind `#[cfg(feature = "interior_visibility")]` so these methods simply don’t exist in the public API when the feature is off.

5) Client-side API surface: exposing local-entity lookups on Client
   - File layout:
       - Add `client/src/interior_visibility.rs`.
       - In `client/src/lib.rs`:
           - Add `#[cfg(feature = "interior_visibility")] mod interior_visibility;`
           - Add `#[cfg(feature = "interior_visibility")] pub use interior_visibility::*;`
           - Optionally: `#[cfg(feature = "interior_visibility")] pub use naia_shared::world::local::LocalEntity;`
   - In `client/src/interior_visibility.rs`:
       - `use` the types needed:
           - `use naia_shared::world::local::LocalEntity;`
           - `use crate::{Client, EntityRef, EntityMut};` (or correct paths).
           - Internal world/replication objects that store LocalEntity → entity mappings on the client.
       - Add an `impl` block for `Client`:
           - `pub fn local_entities(&self) -> Vec<LocalEntity>`
               - Returns the set of LocalEntity IDs that currently exist on the client (i.e., all entities replicated to this client).
           - `pub fn local_entity(&self, local: LocalEntity) -> EntityRef<'_>`
               - Resolves a LocalEntity into an `EntityRef` pointing at the client-side ECS entity.
           - `pub fn local_entity_mut(&mut self, local: LocalEntity) -> EntityMut<'_>`
               - Same, but with mutable access.
       - As on the server:
           - Use the shared crate helpers from step 3 to translate LocalEntity ↔ global entity ID.
           - Use existing internal APIs to go from global entity ID to `EntityRef` / `EntityMut`.
       - Gate the entire module and the `impl` block behind `#[cfg(feature = "interior_visibility")]`.

6) Semantics and invariants for LocalEntity and the new APIs
   - Define LocalEntity semantics in documentation (in `shared/src/world/local/interior_visibility.rs` or in crate docs):
       - LocalEntity is a per-connection 16-bit identifier assigned to each replicated entity.
       - On the server:
           - The LocalEntity namespace is per-UserKey.
           - `Server.local_entities(user_key)` only returns IDs that are currently live (i.e., entities still replicated to that user and not fully torn down).
       - On the client:
           - The LocalEntity namespace is per-client instance.
           - `Client.local_entities()` only returns IDs that are currently live on that client.
   - Clarify lifecycle:
       - LocalEntity IDs may be reused after an entity is fully removed and its replication state is cleaned up (if this is how the underlying implementation currently behaves).
       - New APIs are meant for inspection/testing, not for long-term stable IDs across sessions.
   - Error-handling policy:
       - Decide on a consistent strategy aligned with existing APIs:
           - Either:
               - The new methods panic on invalid LocalEntity (matching other “must exist” APIs), or
               - They return Result/Option and the signatures are adjusted accordingly.
       - Whatever policy is chosen, document it and apply it consistently on both Client and Server.

7) Keep all new logic contained within the interior_visibility modules
   - Only modifications outside the new `interior_visibility.rs` files:
       - Cargo feature definitions (root and per-crate).
       - `mod` declarations + `pub use` in existing `lib.rs` / `mod.rs` files.
       - Optional documentation references (e.g., entries in `FEATURES.md`).
   - All actual logic:
       - Mapping helpers in `shared/src/world/local/interior_visibility.rs`.
       - `impl Server` and `impl Client` methods in `server/src/interior_visibility.rs` and `client/src/interior_visibility.rs` respectively.
   - This keeps feature-specific “interior visibility” concerns completely isolated from the main replication logic and makes it easy to strip or audit.

8) Tests and validation strategy
   - Shared-level tests:
       - Under `shared/tests/` or `shared/src/world/local/tests`, when the feature is enabled:
           - Create a minimal fake/simulation of the LocalEntity mapping using the existing world/replication structs.
           - Verify that:
               - LocalEntity values are generated and associated with entities upon replication.
               - Helper functions correctly map LocalEntity ↔ global entity ID.
   - Server-level tests:
       - Under `server/tests/` with `#![cfg(feature = "interior_visibility")]`:
           - Spin up a minimal Server + one User.
           - Spawn one or more entities, make them in-scope for the user.
           - Assert:
               - `server.local_entities(user_key)` returns the expected set of LocalEntity IDs.
               - For each `local` in that set, `server.local_entity(user_key, local)` and `server.local_entity_mut(user_key, local)` point to entities that are also visible via the existing Server APIs.
   - Client-level tests:
       - Under `client/tests/` with `#![cfg(feature = "interior_visibility")]`:
           - Run a minimal client/server integration using existing test harness patterns.
           - After entities are replicated:
               - Verify `client.local_entities()` contains LocalEntity IDs.
               - Verify each ID resolves to valid `EntityRef` / `EntityMut`.
   - E2E harness integration:
       - Document for downstream users (like your harness) that they must enable the `interior_visibility` feature on `naia`, `naia-server`, and `naia-client` in their Cargo.toml to access these methods.

9) Documentation and discoverability
   - Update `FEATURES.md` in the root repository:
       - Add an entry for `interior_visibility` describing:
           - Intent: “Testing / E2E harness only; exposes LocalEntity-based visibility into internal replication state.”
           - Scope: “Adds LocalEntity alias and Client/Server APIs for LocalEntity → EntityRef/EntityMut lookup.”
           - Stability caveat: “APIs may change between minor releases; not intended for general application code.”
   - Add doc comments on all public items in the new modules:
       - Explain what LocalEntity is and that it is per-connection, ephemeral, and intended mainly for introspection.
       - Explain that the new methods are only available when the `interior_visibility` feature is enabled.
   - If the project uses `doc_cfg` for feature-gated APIs, annotate the new methods and the LocalEntity alias accordingly for better docs.rs integration.

This plan should give a Cursor agent enough structure to:
   - Wire the Cargo features correctly.
   - Create and isolate `interior_visibility.rs` modules in each crate.
   - Reuse existing replication bookkeeping for LocalEntity mapping.
   - Expose the exact Client/Server APIs you specified, only when the feature is enabled.
