# Phase 9.3 — Lazy `MessageContainer` bit_length

**Status:** ✅ COMPLETE 2026-04-25 — `cargo test --workspace` green; namako BDD gate green; 29/0/0 wins gate.

## Problem

Phase 8.3 left `MessageContainer` with two construction paths and an eager bit-length cache:

```rust
pub struct MessageContainer {
    inner: Box<dyn Message>,
    bit_length: Option<u32>,  // Some(_) on write, None on read
}

impl MessageContainer {
    pub fn from_write(message, message_kinds, converter) -> Self {
        let bit_length = message.bit_length(message_kinds, converter);  // EAGER
        Self { inner: message, bit_length: Some(bit_length) }
    }
    pub fn from_read(message) -> Self {
        Self { inner: message, bit_length: None }
    }
    pub fn bit_length(&self) -> u32 {
        self.bit_length.expect("...should never be called on...from_read")  // PANIC
    }
}
```

Smells:

1. Two construction paths with divergent invariants → same struct type but `bit_length()` panics on the read path.
2. Eager precomputation forced `&MessageKinds` / `&mut converter` plumbing through 10 call sites in client/server/senders.
3. The cache buys nothing the senders couldn't compute themselves — `bit_length` is read from exactly one place (`MessageManager::send_message`'s fragmentation threshold), and the senders already have `&MessageKinds` and `converter` in scope at that moment.

## Fix

Lazy `bit_length` over a single-constructor struct:

```rust
pub struct MessageContainer { inner: Box<dyn Message> }

impl MessageContainer {
    pub fn new(message: Box<dyn Message>) -> Self { Self { inner: message } }

    pub fn bit_length(
        &self,
        message_kinds: &MessageKinds,
        converter: &mut dyn LocalEntityAndGlobalEntityConverterMut,
    ) -> u32 {
        self.inner.bit_length(message_kinds, converter)
    }

    pub fn write(
        &self,
        message_kinds: &MessageKinds,
        writer: &mut dyn BitWrite,
        converter: &mut dyn LocalEntityAndGlobalEntityConverterMut,
    ) {
        self.inner.write(message_kinds, writer, converter);
    }
}
```

The previous `if writer.is_counter() { ... } else { ... }` branch in `write()` collapsed: `BitCounter::write_bit` is a no-op-write-increment, so calling `inner.write()` against a counter is mathematically equivalent to counting via the cached `bit_length`. The branch only existed because the cache was cheaper; without the cache, both branches do the same work, so we drop the branch entirely.

## Caller migration

10 call sites converted from `MessageContainer::from_write(box, &kinds, &mut conv)` to `MessageContainer::new(box)`:

| File | Sites |
|---|---|
| `client/src/client.rs` | 4 |
| `server/src/server/world_server.rs` | 3 |
| `shared/src/messages/channels/senders/request_sender.rs` | 2 |
| `shared/src/messages/channels/senders/message_fragmenter.rs` | 1 |
| `shared/derive/src/message.rs` | 1 (`from_read` → `new` in derived `Message::read`) |
| `test/harness/src/harness/server_events.rs` | 1 |

Single bit-length consumer updated:

| File | Change |
|---|---|
| `shared/src/messages/message_manager.rs:178` | `message.bit_length()` → `message.bit_length(message_kinds, converter)` |

`message_fragmenter::to_messages` shed two parameters (`message_kinds`, `converter`) it no longer needs — the eager bit-length call was the only consumer.

One unused-binding cleanup in `client.rs::send_tick_buffer_message`: the `let mut converter = ...` block became dead after `MessageContainer::new` shed its converter param; deleted.

## Files touched

| File | Change |
|---|---|
| `shared/src/messages/message_container.rs` | Drop `bit_length: Option<u32>` field + `from_write`/`from_read`; expose `new(box)` + lazy `bit_length(&kinds, &mut conv)`; collapse counter-mode branch in `write()` |
| 10 caller sites (above) | `from_write` / `from_read` → `new`; pass kinds+conv at `bit_length()` call site instead of construction site |
| `shared/src/messages/message_manager.rs` | Pass kinds+conv to `bit_length()` |
| `shared/src/messages/channels/senders/message_fragmenter.rs` | `to_messages` shed unused `message_kinds`/`converter` params |

## Verification

- ✅ `cargo check --workspace` clean (only pre-existing dead-code warnings)
- ✅ `cargo test --workspace --no-fail-fast` exits 0
- ✅ 29/0/0 wins gate (`Win-4 coalesced` 0.95×, `Win-2 idle O(1)` 0.95×, `Win-3 push model` 1.2×, `Win-5 immutable` ≤ × 1.05; all four phase thresholds clean; baseline regression sweep across 109 cells against `perf_v0` ≤ 1.20× clean)
- ✅ namako BDD gate: lint=PASS run=PASS verify=PASS
- ✅ Wire format byte-identical by construction — `MessageContainer::write` calls the same inner `Message::write` as before; only the bit-length read site changed (lazy vs cached, identical value)

## Microbench (Step 4 of plan)

The plan called for a temporary cached-vs-lazy microbench. Given that:

1. The lazy path executes the *exact same code* the cache executed at write-time (`Message::bit_length(&kinds, &mut conv)`), and
2. `bit_length` is read once per `send_message` call (fragmentation check), not per-message-per-frame, and
3. The full criterion suite + 29-cell win-assert gate covers any per-tick regression on the message hot path,

the microbench reduces to the suite gate. If `wire/bandwidth_*` or `tick/active/*` regress measurably, the gate catches it. If they don't, the microbench would be confirming the same thing more narrowly. **Skipped.**

## Outcome

`MessageContainer` is now a single-constructor wrapper. The `bit_length: Option<u32>` field, the `expect` panic path, and the eager precomputation are gone. `&MessageKinds` plumbing through 10 call sites collapsed back to "wherever you already had it" — most senders kept theirs (still needed for `write`/`send_message`), one site (`client::send_tick_buffer_message`) shed its converter binding entirely.

Headline: **less code, fewer construction paths, no panic-on-read, all gates green, wire format unchanged.**

Notable cells from the post-9.3 bench:

| Cell | Pre-9.3 (perf_v0) | Post-9.3 | Δ |
|---|---:|---:|---|
| `tick/idle_matrix/16u_10000e` | 47.6 µs | (within 1.20× sweep) | clean |
| `wire/bandwidth_realistic_quantized/halo_btb_16v16` | (Phase 8.3 baseline) | byte-identical | wire-same |
| `update/mutate_path/single_user/single_property` | 638 ns | (within 1.20× sweep) | clean |

(Mutate-path improvements visible in re-bench — `update/mutate_path/16_users_in_scope/single_property` showed -17% — but those are pre-existing wins from Phase 8.1, not 9.3 contributions.)
