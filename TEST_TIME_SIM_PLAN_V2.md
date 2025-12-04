# Naia Test Time Simulation Refactor Plan  
*(Option 1: Test Backend for `Instant` in `naia_socket_shared`)*

---

## 1. Goals and Constraints

**Goals**

- Make all time-dependent behavior in Naia **deterministic** and **fast-forwardable** for tests:
  - E2E scenarios and harness logic
  - RTT/ping, tick scheduling, key recycling, etc.
- Require **zero API changes** for production users:
  - No new parameters in `Server`, `Client`, `TimeManager`, `GameInstant`, etc.
- Allow tests to advance “game time” in **discrete steps** (e.g., `16ms` per tick) from the test harness.

**Constraints / Principles**

- Production builds must continue to use existing backends (`native`, `wasm`, `miniquad`) unchanged.
- The test time system should be **opt-in via a feature flag** (e.g., `test_time`).
- When the test backend is active:
  - `Instant::now()` should always use the **simulated clock**.
  - If the simulated clock is not initialized, this is a **test bug** → fail fast (panic).
- Keep the simulated clock implementation **simple and cheap**:
  - Monotonic integer millisecond counter (e.g., `u64`), backed by a small global or thread-local state.
  - No real wall-clock correlation or complex structures.

---

## 2. High-Level Design Overview

1. **Backend-based approach**  
   - Reuse the existing backend selection in `naia_socket_shared` (`cfg_if!` in `socket/shared/src/backends/mod.rs`).
   - Add a new backend module (e.g., `backends/test`) that defines its own `Instant`.
   - When `feature = "test_time"` is enabled, this backend is selected instead of the usual platform backend.

2. **Simulated clock**  
   - Represent test time as a single monotonically increasing value in **milliseconds since test start**.
   - Provide a minimal test-only API:
     - Initialize the clock with a starting value (usually `0`).
     - Advance the clock by a specified `delta_ms`.
   - `Instant::now()` in the test backend reads from this global/test clock.

3. **Integration with Naia**  
   - Production crates (`naia_server`, `naia_client`, `naia_shared`) are **unaware** of the fake clock.
   - The test harness crate (E2E tests) depends on `naia_socket_shared` with `features = ["test_time"]` and drives the clock:
     - Initialize clock once per test or per scenario.
     - Advance clock on each simulated tick before running server/client processing.

4. **Concurrency policy**  
   - Initially treat E2E tests as **single-threaded**:
     - Either by test runner configuration or by design (one scenario per process).
   - Clock can be a simple global (e.g., an atomic integer).
   - If concurrent E2E tests are needed later, a future enhancement can introduce thread-local clocks.

---

## 3. Phase 1 – Audit and Confirm Time Usage

**Objective:** Confirm all time-sensitive code flows through `naia_socket_shared::Instant`, and that no direct `std::time` usage remains in core networking logic.

**Tasks**

1. Search the codebase for time usage:
   - `Instant::now()`
   - `GameInstant::new(...)`
   - Any remaining direct uses of `std::time::Instant` and `std::time::SystemTime` in Naia’s networking/game-time code.
2. Verify key consumers:
   - On server: `TimeManager`, `Server::process_all_packets`, `Server::take_tick_events`, ping/pong logic, reliability, etc.
   - On client: `BaseTimeManager`, `Client::process_all_packets`, `Client::take_tick_events`, interpolation/extrapolation.
   - Shared: `GameInstant`, `KeyGenerator` and any timeout-related logic.
3. For any stray time usages that don’t go through `naia_socket_shared::Instant`, refactor them to do so (without changing public APIs).

**Exit criteria**

- All Naia timing in the networking/game-time layer is using the `Instant` abstraction from `naia_socket_shared`.

---

## 4. Phase 2 – Introduce `test_time` Backend in `naia_socket_shared`

**Objective:** Add a compile-time-selectable backend that provides a fake `Instant` based on a simulated clock.

**Tasks**

1. **Create test backend module structure**
   - Add a new directory: `socket/shared/src/backends/test/`.
   - Inside, add module files for the test `Instant`:
     - `backends/test/mod.rs`
     - `backends/test/instant.rs` (or equivalent).

2. **Wire up backend selection**
   - In `socket/shared/src/backends/mod.rs`:
     - Extend the `cfg_if!` chain to select the test backend when `feature = "test_time"` is enabled.
     - Ensure this branch is checked **before** platform-specific branches so that `test_time` always wins when enabled.
   - Verify that:
     - When `test_time` is **on**, the `test` backend’s `Instant` is the *only* `Instant` used in that build.
     - When `test_time` is **off**, behavior is unchanged and platform backends are used as today.

3. **Design of test `Instant` (high-level)**
   - `Instant` stores an integer millisecond count from the test clock (e.g., `millis_since_start: u64`).
   - `Instant::now()`:
     - Reads the current millisecond value from the simulated clock.
     - Constructs an `Instant` with that value.
     - If the simulated clock has not been initialized, **panic with a clear message** (since this is a test-only backend and this indicates a test harness error).
   - All existing `Instant` methods are implemented in terms of this `u64` count:
     - Duration computations via subtraction/saturating subtraction.
     - `add_millis`, `subtract_millis`, and any comparison methods (`is_after`, `until`, etc.).

**Exit criteria**

- `naia_socket_shared` builds with and without `feature = "test_time"`.
- When `test_time` is enabled, only the test `Instant` backend is compiled and used.

---

## 5. Phase 3 – Implement the Simulated Clock API

**Objective:** Expose a small, test-only API in `naia_socket_shared` for initializing and advancing the simulated clock.

**Tasks**

1. **Define minimal public API (behind `test_time` feature)**
   - Functions along the lines of:
     - `init_test_clock(initial_ms: u64)`
     - `advance_test_clock(delta_ms: u64)`
   - These functions live in the test backend module or a separate `test_time` module and are only compiled when the `test_time` feature is active.

2. **Clock storage choice**
   - Use a simple global or static value to hold the current simulated time:
     - e.g., an `AtomicU64` or similar atomic integer.
   - Characteristics:
     - Monotonic: `advance_test_clock` must never decrease the stored value.
     - Fast: `Instant::now()` is a simple atomic load.
   - No linkage to wall-clock time; this is purely logical test time.

3. **Enforce explicit initialization**
   - `init_test_clock(initial_ms: u64)`:
     - Must be called once at the start of each test or scenario before any `Instant::now()` calls.
     - If `Instant::now()` is called before initialization:
       - Panic with a clear error message indicating that the test harness must call `init_test_clock` first.
   - `advance_test_clock(delta_ms: u64)`:
     - Adds `delta_ms` to the current clock value.
     - Should be safe to call many times.

4. **Responsibility boundaries**
   - `naia_socket_shared`:
     - Owns and implements the clock storage and the test-only API.
   - Test harness:
     - Is responsible for `init_test_clock` and `advance_test_clock` calls at appropriate times.
   - No production code path should call these APIs.

**Exit criteria**

- A small, clear test-time API exists and compiles only with `feature = "test_time"`.
- The test `Instant` backend uses the simulated clock exclusively.

---

## 6. Phase 4 – Wire the E2E Test Harness to the Simulated Clock

**Objective:** Make the Naia test harness control simulated time through the new API, so that all `Instant::now()` calls in tests see the same deterministic time.

**Tasks**

1. **Update test harness crate dependencies**
   - In the E2E / harness crate’s `Cargo.toml`:
     - Depend on `naia_socket_shared` with `features = ["test_time"]`.
   - Ensure production binaries and libraries do **not** enable `test_time`.

2. **Initialize clock within each test scenario**
   - In the harness’s `Scenario` or equivalent test driver:
     - On scenario creation, call `init_test_clock(0)` (or another known baseline).
     - Optionally allow configuration of the initial time if needed, but default to `0` ms is typically enough.

3. **Advance clock per tick**
   - Define a per-tick duration (e.g., `16ms` for ~60 FPS) in the scenario.
   - In `Scenario::tick_once()` (or equivalent):
     - First, call `advance_test_clock(tick_ms)`.
     - Then run server and client update functions that internally call `Instant::now()`:
       - `TimeManager::recv_server_tick`
       - `Server::process_all_packets`
       - `Client::process_all_packets`
       - Any polling, sending, etc.

4. **Consistency guarantees**
   - Ensure all operations inside a given `tick_once()` see a **consistent** snapshot of time:
     - Time only changes when `advance_test_clock` is called.
   - If additional “sub-steps” per tick are needed in the future (e.g., multiple phases per tick), define a clear policy for when the clock increments (per phase vs. per top-level tick).

5. **Potential test APIs**
   - For tests that need finer control, expose helper methods in the harness:
     - `tick_for_n_steps(n: u32)`
     - `advance_time_ms(ms: u64)` (which internally calls `advance_test_clock` and runs zero or more ticks depending on semantics).

**Exit criteria**

- All E2E tests that rely on time now get their timing from the simulated clock.
- The harness is the **only** component deciding how and when time advances.

---

## 7. Phase 5 – Validate Integration with Core Components

**Objective:** Confirm that all higher-level components behave correctly and deterministically under test time.

**Tasks**

1. **`GameInstant`**
   - Confirm `GameInstant::new` ultimately uses `Instant::now()` from the test backend (via existing code path).
   - Validate:
     - Ping/pong and RTT calculations are consistent across runs.
     - Any logic based on elapsed game time behaves correctly when time jumps by fixed increments.

2. **Tick management (`TimeManager`, server/client tick events)**
   - Run tests where:
     - The tick interval is known (e.g., 50ms).
     - The scenario advances time exactly at that interval.
   - Verify:
     - Server and client tick events fire when expected.
     - No off-by-one or drift between server and client.

3. **Key recycling (`KeyGenerator`)**
   - Ensure:
     - Keys are recycled only after the expected simulated timeouts.
     - There are no underflow/overflow issues when computing time differences with `u64` millis.

4. **Edge conditions**
   - Tests where:
     - Time is advanced by large jumps (e.g., seconds or minutes in one step).
     - Time is advanced in small increments around boundary conditions (e.g., just before and just after tick or timeout thresholds).
   - Confirm that all behavior is deterministic and repeatable.

**Exit criteria**

- All time-based tests pass reliably across many runs.
- No timing-related flakiness remains in the harness.

---

## 8. Phase 6 – Concurrency, Configuration, and Documentation

**Objective:** Finalize the operational model for running tests and document the new behavior.

**Tasks**

1. **Test concurrency model**
   - Decide short-term policy:
     - Either run E2E tests with a single test-thread (e.g., by configuration or by grouping E2E tests into a single binary), **or**
     - Explicitly document that `test_time` is not safe for concurrent E2E tests yet.
   - If single-threaded:
     - Confirm that tests don’t accidentally run in parallel when using the simulated clock.
   - If concurrency is desired later:
     - Plan a follow-up refactor to make the test clock thread-local and provide one clock per test.

2. **Configuration knobs**
   - Provide test harness options to:
     - Configure tick duration (default `16ms`).
     - Possibly configure initial time or multiple phases per tick if needed.

3. **Documentation**
   - Add developer docs / comments summarizing:
     - The existence and purpose of the `test_time` feature.
     - The required pattern: `init_test_clock` → `advance_test_clock` → run scenario.
     - The fact that production builds must not enable `test_time`.
   - Include short examples in the test harness docs on how to write time-dependent tests using the new system.

**Exit criteria**

- E2E test execution model is clear and documented.
- Contributors know how to use the `test_time` feature and the simulated clock from tests.

---

## 9. Final Success Criteria Checklist

- [ ] Production builds (`naia_server`, `naia_client`, `naia_shared`) **unchanged** and continue to use platform backends for `Instant`.
- [ ] `naia_socket_shared` has a `test_time` backend:
  - [ ] Selected by feature flag.
  - [ ] Implemented using a simple monotonic millisecond clock.
- [ ] New test-only API exists and is used:
  - [ ] `init_test_clock` is required before any `Instant::now()` calls in tests.
  - [ ] `advance_test_clock` is used to move simulated time forward.
- [ ] All time usage in Naia’s core networking/game-time logic goes through `naia_socket_shared::Instant`.
- [ ] E2E tests use the simulated clock consistently:
  - [ ] Deterministic tick-based time progression.
  - [ ] No timing-based flakiness or underflow/overflow issues.
- [ ] Concurrency model is defined and documented (initially single-threaded for E2E with test time).
- [ ] Contributors can easily reason about test time vs. real time and extend tests accordingly.
