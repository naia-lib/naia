# naia_npap

NPAP (Namako Project Adapter Protocol) adapter for Naia BDD tests.

This binary implements the adapter protocol that allows the Namako engine to execute Naia's step bindings.

## Usage

```bash
# Print the semantic step registry (for namako lint)
cargo run -p naia_npap -- manifest

# Execute a resolved plan (for namako run)
cargo run -p naia_npap -- run -p resolved_plan.json -o run_report.json
```

## Architecture

```
┌─────────────────────────────────────────────────────────────┐
│  Namako Engine (namako-cli)                                 │
│    ├── lint → calls `naia_npap manifest`                    │
│    └── run  → calls `naia_npap run --plan ... --out ...`    │
└─────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────┐
│  naia_npap (this crate)                                     │
│    ├── manifest.rs  → Emits step registry JSON              │
│    └── run.rs       → Executes plan by binding_id dispatch  │
└─────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────┐
│  naia_tests                                                 │
│    └── Step bindings (Given/When/Then functions)            │
│          └── Uses naia_test_harness::Scenario               │
└─────────────────────────────────────────────────────────────┘
```

## Commands

### `manifest`

Emits the semantic step registry as JSON. The registry includes:
- All registered step bindings (Given/When/Then)
- Binding IDs (derived from kind + expression)
- Signature metadata (captures, docstring, datatable)
- Implementation hashes (for drift detection)

### `run`

Executes a resolved plan produced by `namako lint`:
1. Validates `step_registry_hash` matches current manifest (refuses stale plans)
2. Dispatches steps by `binding_id` only (no text matching at runtime)
3. Produces a `run_report.json` with execution evidence

---

## Execution Model

All step functions are **synchronous**. The adapter uses sync-compatible APIs from `naia_test_harness`.

### Why Synchronous?

1. **Simplicity** — No runtime management complexity in step bindings
2. **Determinism** — Execution order is predictable, no async race conditions
3. **Compatibility** — `naia_test_harness::Scenario` provides sync APIs
4. **No nested runtime** — Avoids tokio runtime-in-runtime panics

### Step Execution

```
for each scenario:
    world = TestWorld::new()
    for each step in plan:
        dispatch_by_binding_id(step, &mut world)  ← sync, blocking
```

Each step function:
- Receives `&mut TestWorldMut` (Given/When) or `&TestWorldRef` (Then)
- Executes synchronously to completion
- Uses `Scenario` APIs for server/client orchestration
- Returns normally on success, panics on failure

### The Scenario Abstraction

`naia_test_harness::Scenario` provides sync APIs:

```rust
// Mutate server/client state
scenario.mutate(|ctx| { ... });

// Wait for a condition (tick-based polling)
scenario.until(500.ticks()).expect_msg("condition", |ctx| { ... });
```

The harness internally manages async runtimes — step code never touches them.

### Forbidden Patterns

❌ **Never create a tokio runtime inside a step:**
```rust
#[when("something happens")]
fn bad_step(ctx: &mut TestWorldMut) {
    // FORBIDDEN: nested runtime panic
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async { ... });
}
```

✅ **Use the Scenario API:**
```rust
#[when("the server disconnects the client")]
fn disconnect_client(ctx: &mut TestWorldMut) {
    ctx.disconnect_last_client();  // Uses Scenario internally
}
```

---

## Error Handling

| Situation | Behavior |
|-----------|----------|
| Step panic | Scenario marked `Failed`, captured in run report |
| Assertion failure | Same as panic |
| Then step returns `Pending` | Polling continues until `Passed` or timeout |
| Then step returns `Failed(msg)` | Immediate failure, no retry |

---

## Teardown

Each scenario gets a fresh `TestWorld::default()`. The `Scenario` is created in Given steps and dropped when the scenario completes. No explicit teardown hooks needed.

---

## Related Crates

| Crate | Purpose |
|-------|---------|
| `naia_tests` | Step binding functions (Given/When/Then) |
| `naia_test_harness` | Test scenario orchestration |
| `namako` | Engine/CLI, resolution, verification |
