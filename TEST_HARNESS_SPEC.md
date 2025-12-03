# Scenario Test Harness API Spec

This spec defines a high-level, opinionated test harness API for end-to-end tests of a client/server networking library (e.g. `naia`). It is designed to be implemented inside the `naia_test` crate and used by integration tests to express behaviors in terms of **logical entities**, **client roles**, and **mutate/expect** phases.

The spec is intentionally high-level and does not prescribe internal data structures beyond what is necessary for the public API and semantics. A Cursor agent should implement the described API and accompanying tests to validate its behavior.

---

## 1. Goals

- Provide a clean, expressive API for E2E tests that follow the rhythm:

  - **Mutate phase**: perform actions on server and clients.
  - **Expect phase**: advance the simulation until all expectations pass (or timeout).

- Hide low-level mechanics (instant, ticking, local transport, per-actor entity IDs) behind a small, consistent surface.

- Allow tests to talk about entities via a **logical key** (`EntityKey`) that is independent of per-actor IDs.

- Allow tests to reference clients by a **compact key** (`ClientKey`), not string labels.

- Make replication / delegation / authority scenarios readable and stable.

---

## 2. High-Level Concepts

### 2.1 Scenario

`Scenario` is the top-level harness object used by tests. It:

- Owns a single in-process server and multiple in-process clients.
- Manages a deterministic or pseudo-deterministic ticks-based simulation.
- Internally uses local transport / test sockets (no real network).
- Maintains a registry that maps a logical `EntityKey` to per-actor entity IDs.
- Provides explicit phases:

  - `server_start()` – boot the server.
  - `client_connect()` – connect a client to the server and register it with a `ClientKey`.
  - `mutate(|MutateCtx| { ... })` – perform actions.
  - `expect(|ExpectCtx| { ... })` – assert on outcomes; repeatedly tick until all expectations succeed or a max tick count is reached.

### 2.2 ClientKey

- A small, copyable key assigned by `Scenario::client_connect`.
- Used to identify clients throughout tests.
- Think of it as an index into `Scenario`’s client list (but implementation is up to the agent).

### 2.3 EntityKey

- A small, copyable key representing a **logical game entity** in the test.
- Returned by a `spawn().track()` call during a `MutateCtx` client action.
- Internally mapped to server-side and client-side entity IDs as replication progresses.
- Tests only ever see this logical key; they never see per-actor entity IDs.

### 2.4 Entity Registry (behavior)

Internally, `Scenario` maintains an entity registry that:

- On `track()`:

  - Allocates a new `EntityKey`.
  - Records the spawning client’s local entity ID for that key.
  - Optionally stores a simple matcher for discovering the same logical entity on server and other clients.

- On `expect` methods:

  - Fills in missing per-actor IDs for a given `EntityKey` as entities appear on server/clients (e.g., first entity matching some predicate).

- Must ensure that once a per-actor ID is associated with `EntityKey` for a given `ClientKey` or server, that association is stable for the lifetime of the scenario.

Implementation of the registry and matchers is up to the agent; it only needs to be deterministic and sufficient for the tests described below.

---

## 3. Public API Summary

All types and functions listed here are intended as public or crate-visible API for tests.

### 3.1 Types

- `struct Scenario`
- `struct ClientKey` (copyable, comparable)
- `struct EntityKey` (copyable, comparable)

Context types (scoped to the harness):

- `struct MutateCtx<'a>`
- `struct ExpectCtx<'a>`
- `struct ServerMutateCtx<'a>`
- `struct ClientMutateCtx<'a>`
- `struct ServerExpectCtx<'a>`
- `struct ClientExpectCtx<'a>`
- Entity views/builders used inside contexts:
  - `struct SpawnBuilder<'a>`
  - `struct ClientEntityMut<'a>`
  - `struct ClientEntityExpect<'a>`

The exact module layout is up to the agent; a reasonable structure is:

- `naia_test::harness::scenario::Scenario`
- `naia_test::harness::keys::{ClientKey, EntityKey}`
- `naia_test::harness::ctx_mutate::*`
- `naia_test::harness::ctx_expect::*`

### 3.2 Scenario lifecycle and setup

Required methods on `Scenario`:

- `fn new(protocol: naia_shared::Protocol) -> Scenario`

  - Creates a fresh scenario.
  - Does not start the server or create any clients yet.

- `fn server_start(&mut self)`

  - Initializes the server and its listening socket using the given protocol.
  - Creates a default “main” room (RoomKey) internally.
  - Must be called exactly once before any clients connect.
  - Idempotent behavior (calling twice) may panic or be disallowed; tests should call it once.

- `fn client_connect(&mut self, auth: Auth, display_name: &str) -> ClientKey`

  - Creates a new client, assigns it a `ClientKey`.
  - Performs handshake/auth with the server and joins the main room (or whatever is appropriate for your test setup).
  - Returns the `ClientKey` for use in `MutateCtx` / `ExpectCtx`.
  - Order of calls defines client ordering but not semantics.

### 3.3 Mutate phase API

`Scenario::mutate`:

- `fn mutate<R>(&mut self, f: impl FnOnce(&mut MutateCtx) -> R) -> R`

Behavior:

- Creates a `MutateCtx` view over the scenario.
- Executes the closure exactly once.
- After the closure returns, `Scenario` **must tick** the simulation at least once to process any queued actions (e.g., send/receive messages, replication).
- Implementation may choose to tick exactly once or a small fixed number; tests should not rely on that detail.

`MutateCtx` provides:

- `fn server<R>(&mut self, f: impl FnOnce(&mut ServerMutateCtx) -> R) -> R`

  - Gives a mutable server context for this mutate-phase.
  - May be called multiple times within a single `mutate`.

- `fn client<R>(&mut self, client: ClientKey, f: impl FnOnce(&mut ClientMutateCtx) -> R) -> R`

  - Gives a mutable client context for the given `ClientKey`.
  - May be called multiple times within a single `mutate`.

#### ServerMutateCtx requirements

Minimal required operations (expandable later):

- `fn include_in_scope(&mut self, client: ClientKey, entity: EntityKey)`

  - Ensure that the logical entity identified by `EntityKey` is in scope for the given client.
  - Internally uses the server-side entity ID associated with `EntityKey`.

(Additional server-side mutating operations can be added later as needed; for this spec, only scope management is required.)

#### ClientMutateCtx requirements

Operations on the client’s world:

- `fn spawn(&mut self) -> SpawnBuilder<'_>`

  - Begin spawning a new entity on this client.
  - Returns a builder that can be customized and then tracked as a logical `EntityKey`.

- `fn entity(&mut self, entity: EntityKey) -> ClientEntityMut<'_>`

  - Create a mutable view over a previously tracked logical entity for this client.
  - Internally looks up the client-side entity ID associated with `EntityKey`.

`SpawnBuilder` must support:

- `fn with_position(self, position: Position) -> Self`

  - Attach a `Position` component to the new entity.
  - More generic `with_component<T>` could be added; for now, position is enough.

- `fn track(self) -> EntityKey`

  - Finalizes the spawn builder.
  - Creates the actual entity in this client’s world.
  - Allocates a new `EntityKey` for this logical entity.
  - Registers the spawning client’s local entity ID in the entity registry.
  - Returns the `EntityKey` to the test.

`ClientEntityMut` must support at least:

- `fn delegate(self)`

  - Configure replication for this entity to use delegated/authority-based replication on this client.
  - Equivalent to setting `ReplicationConfig::Delegated`.

- `fn request_auth(self)`

  - Request authority over this entity from the server.

- `fn release_auth(self)`

  - Release authority over this entity back to the server.

- `fn set_position(self, position: Position)`

  - Set/update the position of the entity on this client.

(Additional mutating methods such as `set_component<T>` can be added later.)

---

## 4. Expect phase API

`Scenario::expect`:

- `fn expect(&mut self, f: impl FnOnce(&mut ExpectCtx))`

Behavior:

- Creates an `ExpectCtx` with a default maximum tick budget (e.g., 50 ticks).
- The closure `f` should **only register expectations**, not mutate the scenario directly.
- After `f` returns, `Scenario` enters a loop:

  - For each tick:
    - Advance the simulation by one tick.
    - Evaluate all registered expectations.
    - If all expectations pass in the same tick, `expect` returns successfully.
    - If not all expectations pass within `max_ticks`, `expect` panics with a descriptive error describing which expectations failed.

- Implementation should ensure that expectations are checked after each tick and never re-run actions.

`ExpectCtx` provides:

- `fn ticks(&mut self, max_ticks: usize)`

  - Override the default maximum tick budget for this expect phase only.

- `fn server(&mut self, f: impl FnOnce(&mut ServerExpectCtx))`

  - Register one or more server-side expectations.

- `fn client(&mut self, client: ClientKey, f: impl FnOnce(&mut ClientExpectCtx))`

  - Register one or more client-side expectations for the given client.

Implementations of `ServerExpectCtx` / `ClientExpectCtx` may internally push predicates with labels into a list that `Scenario::expect` evaluates each tick.

### 4.1 ServerExpectCtx requirements

Minimal expectations for current usage:

- `fn has_entity(&mut self, entity: EntityKey)`

  - Expect that the server has replicated/created a concrete entity corresponding to the logical `EntityKey`.
  - If this is the first time server-side mapping for this `EntityKey` is required, the implementation should:

    - Discover the appropriate server entity ID (e.g., first entity, or via a registry matcher).
    - Associate that server entity ID with the `EntityKey` in the entity registry.

- `fn enter_main_room(&mut self, entity: EntityKey)`

  - Expect that the server will (eventually) have the entity in the main room.
  - Either:

    - `MutateCtx`’s server operations should perform the actual `enter_room`, and `enter_main_room` is not needed here, or
    - `enter_main_room` is treated as an expect-time effect with a corresponding predicate.
  
  For simplicity in this spec, assume `enter_main_room` will be used in `MutateCtx` (server) as a pure action, not an expectation. If you keep it as part of `ServerExpectCtx`, it must be purely an expectation (“the entity is now in the main room”), not a mutating operation.

- `fn event<T: 'static>(&mut self, label: &str)`

  - Expect that the server will produce at least one world event of type `T` (e.g. `DelegateEntityEvent`) within the tick budget.
  - The `label` is used for error messages.
  - Implementation should not permanently drain all events without considering future expects; basic draining per expect phase is acceptable for now.

### 4.2 ClientExpectCtx requirements

Key responsibilities:

- Ensure per-client entity mapping is filled when needed.
- Provide simple, high-level expectation helpers for replication/authority/position.

Required methods:

- `fn sees(&mut self, entity: EntityKey)`

  - Expect that this client will eventually see the logical entity corresponding to `EntityKey`.
  - If there is no mapping for this client yet, implementation should:

    - Discover the appropriate client-side entity ID (e.g., first entity, or via `EntityRegistry` matcher).
    - Associate that client entity ID with `EntityKey`.

- `fn entity(&mut self, entity: EntityKey) -> ClientEntityExpect<'_>`

  - Return an expectation view for that logical entity on this client.
  - Must ensure mapping exists (implicitly calling `sees` if needed).

`ClientEntityExpect` must support:

- `fn replication_is_delegated(self)`

  - Assert that the client’s replication configuration for this logical entity is `Delegated`.

- `fn auth_is(self, expected: naia_shared::EntityAuthStatus)`

  - Assert that the client’s authority status for this logical entity eventually equals `expected`.

- `fn position_is(self, expected_x: f32, expected_y: f32)`

  - Assert that the client’s position for this logical entity eventually equals `(expected_x, expected_y)`.

Implementation details (how to read replication/auth/position from the underlying client/world) are up to the agent; these exist already in current tests.

---

## 5. Tick semantics

- `Scenario` must own the ticking mechanism and **tests should never call tick directly** for behavior checks.
- `Scenario::mutate` must tick at least once after actions to propagate immediate effects.
- `Scenario::expect` must:

  - Run a loop up to `max_ticks`:
    - tick
    - evaluate all expectations
    - break when all pass

- Implementation should be deterministic and not use real-time sleeping.

---

## 6. Example Usage Tests (to be implemented)

The agent should add a new test module (e.g. `tests/harness_scenarios.rs`) that uses only the public harness API and validates core behavior.

These tests serve as self-checks for the harness.

### 6.1 Test: single client spawn replicates to server

Name: `harness_single_client_spawn_replicates_to_server`

Flow:

- Create `Scenario::new(protocol())`.
- Call `server_start()`.
- Connect one client:

  - `let a = scenario.client_connect(Auth::new("client_a","password"), "Client A");`

- Mutate phase:

  - `let ent = scenario.mutate(|ctx| { ctx.client(a, |c| c.spawn().with_position(Position::new(1.0, 2.0)).track()) });`

- Expect phase:

  - `scenario.expect(|ctx| { ctx.server(|sv| { sv.has_entity(ent); }); });`

Optional:

- Additional expect: server’s main room contains entity `ent`.

### 6.2 Test: two clients see the same logical entity

Name: `harness_two_clients_entity_mapping`

Flow:

- Setup scenario with server and two clients `a` and `b`.
- In a mutate-phase:

  - Client `a` spawns and tracks an entity `ent`.
  - Server includes `b` in scope for `ent` (via `ServerMutateCtx::include_in_scope`).

- Expect phase:

  - `scenario.expect(|ctx| { ctx.client(b, |c| { c.sees(ent); }); });`

- Additional expect:

  - Assert that server has entity `ent`.
  - Assert that both `a` and `b` report the same logical entity position after a client `a` position change in a subsequent `mutate` + `expect`.

### 6.3 Test: delegating authority from A to B (smoke test)

Name: `harness_delegation_flow_smoke`

Flow (high-level, matching the bug scenario):

1. Setup server + connect clients `a` and `b`.
2. In a mutate-phase, client `a` spawns entity `ent` and sets initial position.
3. Expect:

   - Server has entity `ent`.

4. Mutate: server includes `b` in scope for `ent`.
5. Mutate: client `a` configures `ent` as delegated.
6. Expect:

   - Server sees a `DelegateEntityEvent` for this entity.
   - Client `a` sees replicated entity `ent` as delegated and `auth_is(Granted)`.

7. Mutate: client `a` releases authority.
8. Expect:

   - Client `a` sees `auth_is(Available)`.

9. Expect:

   - Client `b` sees `ent` (via `sees(ent)`).
   - Client `b` sees `ent` as delegated and `auth_is(Available)`.

10. Mutate: client `b` requests authority for `ent`.
11. Expect:

   - Client `b` sees `auth_is(Granted)`.
   - Client `a` still sees `auth_is(Available)`.

12. Mutate: client `b` sets new position for `ent` (e.g., `(100.0, 200.0)`).
13. Expect:

   - Client `a` eventually sees `position_is(100.0, 200.0)` for `ent`.

All of the above should be expressible using only:

- `Scenario::new`, `server_start`, `client_connect`
- `mutate(|MutateCtx| { ... })`
- `expect(|ExpectCtx| { ... })`
- `ServerMutateCtx`, `ClientMutateCtx`
- `ServerExpectCtx`, `ClientExpectCtx`
- `ClientEntityMut`, `ClientEntityExpect`
- `EntityKey`, `ClientKey`

---

## 7. Non-goals / Out of Scope for This Spec

- Real transport tests (UDP/WebRTC) are explicitly out of scope.
- Property-based/fuzz testing is not addressed here.
- Fixtures, rspec-style suites, or cucumber integration are not required as part of this harness.
- This spec does not require any particular logging or tracing format, though clear failure messages from `expect` are encouraged.

---

## 8. Implementation Notes (guidance, not mandates)

- Internals may re-use existing local-transport helpers (e.g., `LocalTransportBuilder`, `update_client_server_at`) but tests should stay decoupled from those details.
- The entity registry and matching logic can start simple (e.g., “first entity” heuristic) and be improved later.
- Prefer clear names and small methods over deep abstractions; the point is test readability.

The Cursor agent’s task is to:

1. Implement the harness API as described.
2. Implement the smoke tests in section 6 using only the harness API.
3. Ensure the tests pass, demonstrating that the harness supports the expected behaviors.