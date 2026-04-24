# Priority Accumulator — Research Distillation

**Status:** 🔎 research stage deliverable (v1 — awaiting Connor's review)
**Companion:** `PRIORITY_ACCUMULATOR_SIDEQUEST.md` (scope, questions, deliverables)

---

## Scope framing — sender-side, not server-side

Before the substance: both accumulators are **sender-side** mechanisms. A lot of Fiedler's writeups (and most third-party discussions, including bevy_replicon #57 and Unity's docs) phrase this as a "server" feature because their examples are server-authoritative games. Naia is not exclusively server-authoritative: it supports client-authoritative messages, requests, responses, and client-owned entities. The accumulator belongs on **whichever peer is assembling outbound packets at a given moment** — the same code path runs on both sides, with identical semantics.

Throughout this doc, read "connection" and "peer" in the symmetric sense. Where specific Naia code paths are named (`server::connection::Connection::send_packets`), there is an analogous client-side path that gets the same treatment.

---

## TL;DR

Glenn Fiedler uses **two distinct accumulator techniques** that are often conflated:

1. **Priority accumulator** (*State Synchronization*, 2015) — a *selection* mechanism for state updates: float-per-object, accumulates each frame, sort by magnitude, fill packet from the top, reset included items, retain the rest.
2. **Bandwidth accumulator** (*Sending Large Blocks of Data*, 2016) — a *pacing* mechanism: `budget += target_bytes_per_sec × dt` per frame, subtract per send, stop when ≤ 0, carry surplus. Token-bucket throttling.

Fiedler's **reliable-ordered-messages** algorithm (2016) uses **neither**. It relies on "oldest unacked first" iteration + a "hasn't been sent in the last 0.1 s" gate + piggyback-until-acked. Per-packet MTU is the only cap.

Naia today mirrors the reliable-message piece (correctly, structurally) but is missing both accumulator techniques. The **bandwidth accumulator** is the direct fix for Phase 4.5's spike. The **priority accumulator** is the long-term fix for component-update fan-out and enables features like FoW-aware replication.

The two are **complementary**: priority selects *what* to include; bandwidth accumulator caps *how much total* goes out per tick.

---

## Primary sources (distilled)

### 1. *State Synchronization* — the priority accumulator proper

> "The solution is to maintain a priority accumulator value per-object that lets us track which objects are the most important to include in the packet over time."

**Data:** one `f32` per replicated object, persistent across frames.

**Per-frame loop:**
1. For each object: `accumulator[i] += priority_function(object[i])`.
2. Sort object indices by accumulator, descending.
3. Walk the sorted list, serialize each object into the packet until the packet is full.
4. For every object *that fit*: `accumulator[i] = 0`.
5. For every object *that did not fit*: leave accumulator alone.

**Guarantees:**
- No starvation — unsent objects compound priority each frame, climbing the sorted list.
- No re-send of unchanged data needed — each packet is self-contained state.
- Adaptive — priority function can be game-specific (player's own cube = 1 000 000; interacting = 100; at-rest = 1).

**Key quote** (on fit logic):
> "After you serialize the packet, reset the priority accumulator to zero for objects that fit but leave the priority accumulator value alone for objects that didn't. This way objects that don't fit in the current packet are first in line to be included in the next packet."

**Scope in Fiedler's article:** strictly state updates (position / orientation / velocity snapshots). Input packets and reliable messages are handled by separate systems.

### 2. *Sending Large Blocks of Data* — the bandwidth accumulator

> "Each frame before you update the chunk sender, take your target bandwidth (eg. 256 kbps), convert it to bytes per-second, and add it multiplied by delta time (dt) to an accumulator."

**Data:** one `f64` (or similar) scalar budget per connection.

**Per-frame loop:**
1. `budget += target_bytes_per_sec × dt`.
2. While `budget > 0` and there is work to do: pick next work item, send it, `budget -= estimated_packet_bytes`.
3. If `budget ≤ 0`, stop; surplus carries into the next frame.

**Numeric examples from the article:**
| target       | bytes/sec | time to send 256 KB |
|--------------|-----------|---------------------|
| 256 kbps     | 32 000    | 8.0 s               |
| 512 kbps     | 64 000    | 4.0 s               |
| 1 mbps       | 125 000   | 2.0 s               |

**Resend trigger (within the same system):**
> "Resend if unacked for `RTT × 1.25`" OR "resend if slice hasn't been sent in the last 100 ms."

**Important:** Fiedler uses this accumulator for *block fragment* pacing, but the technique is generic — any outbound queue can be paced this way. It is the **primitive that pairs with** the priority accumulator.

### 3. *Reliable Ordered Messages* — a counterpoint (what Fiedler did NOT use here)

This article is most structurally similar to Naia's `ReliableMessageSender` / `ReliableSender`. It uses:

- Sequence-indexed **send message buffer** (`message_id → (msg_ptr, bit_count)`).
- Per-outgoing-packet record: `packet_seq → [message_ids included]`.
- Selection: walk from `oldest_unacked` → `current_send_id`, include any message that "hasn't been sent in the last 0.1 s AND fits in the packet."
- On packet ack: remove message_ids from send buffer, advance `oldest_unacked`.

Explicitly *no* priority, *no* bandwidth accumulator — the scheme's per-tick work is bounded only by packet MTU and the "0.1 s" inclusion gate.

**Why this matters:** it demonstrates Fiedler himself scoping the two accumulator techniques to state updates / blocks, not to reliable messages. But that's a 2016 design point for a single-stream protocol. **In Naia, where one connection carries reliable *and* state traffic simultaneously, bandwidth pacing must wrap the union, or reliable bursts starve state (and vice versa).** This is exactly what Steve Streeting documented in Unreal (2025): reliable RPCs gated property replication for 45–60 s under load. See §"Prior art — production failure modes" below.

---

## Prior art — production failure modes

### Halo: Reach — Aldridge / Parsons, GDC 2011 ("I Shot You First")

Shipping AAA netcode that went through the exact design space we're entering. Adds six load-bearing details that my Fiedler-only reading missed:

**1. Priority is per-receiver per-item, not per-item.**
> "Priority is calculated separately per-object per-client. Distance/direction is the core metric. Size & speed affect priority. Shooting & damage apply appropriate boosts. Lots of special cases (e.g. thrown grenades)."

Implication for Naia: the priority function is `f(item, connection)`, not `f(item)`. This matters because the same tile entity has very different priority to two clients at opposite ends of a map. Naia already carries scope / FoW per connection; the priority accumulator extends that model.

**2. "Unreliability enables aggressive prioritization."**
> "Unreliability enables aggressive prioritization, which lets us handle the richness of our simulation."

The mental model: if *everything* is reliable, you can't skip sends, so priority degenerates into "what order do we drain the queue." You need an unreliable tier (or a skip-allowed tier) for priority to actually *work* as a scarcity-allocation mechanism. For Naia this means:

- Component updates (state) are the natural home for the priority accumulator. Newer snapshot > older snapshot; older snapshot is skippable.
- Reliable messages *can't* be dropped by priority, only paced — that's the bandwidth accumulator's job, not the priority accumulator's.
- This sharpens recommendation (1) in the decision list: bandwidth-first is not just a sequencing choice; it is the mechanism that applies to *all* surfaces, while priority only applies cleanly to the skippable ones.

**3. Three traffic tiers, not two.**
> "1. State Data: Guaranteed eventual delivery of most current state. 2. Events: Unreliable notifications of transient occurrences. 3. Control data: High-frequency, best-effort transmission of rapidly-updated data extracted from player input."

Naia today has reliable / unreliable / sequenced. The tier mapping is roughly:
- State Data ↔ `UnorderedReliable` + component updates
- Events ↔ plain messages on reliable channels
- Control data ↔ `SequencedUnreliable` / `UnorderedUnreliable`

The taxonomy is compatible; no new tier needed. But the *treatment* differs per tier: the accumulator should prioritize state updates aggressively (skippable), pace events and control carefully (not skippable without gameplay effect).

**4. Priority functions are a footgun — the idle grenade lesson.**
> "Idle grenades rolling around on the ground had incredibly high network priority. The cause was traced back… to a bugfix at the end of Halo 3! 'Equipment' was given a huge priority boost. Fix: only apply priority boost to active equipment."

A priority function that accumulated inherited boosts across game-state categories produced a shipping bug where bandwidth was dominated by stationary garbage. For Naia:

- The priority function API must be **inspectable at runtime** — you should be able to ask "why is item X at priority Y right now?"
- Telemetry must include **per-item priority histograms** so pathological tails are visible.
- Sane defaults per channel kind; users can override, but "tune priority" is not the default experience.

**5. Network profiling was decisive.**
> "Splice the network profiler data into the films. For the first time, we could analyze network performance after the fact."

They ran *monthly* playtests with traffic shaping and a physical "lag" button players could press to flag moments for engineers. For Naia: extend `bench_instrumentation` counters to emit a per-tick-per-connection record (bytes sent per channel, budget remaining, oldest unsent age, top-N items by priority) that can be dumped to disk and analyzed offline. This is the same class of tool as `idle_distribution.rs` but spanning the accumulator surface.

**6. Concrete bandwidth numbers from shipped netcode.**

| Metric | Value | Notes |
|---|---|---|
| Halo 3 → Reach bandwidth reduction | **to 20% of Halo 3** | priority + encoding + removing duplication |
| Minimum total upstream for 16-player host | **250 kbps** | peer-authoritative topology |
| Bandwidth per replicated biped at combat quality | **~1 kbit/s** | per client per player |
| Minimum packet rate for solid gameplay | **10 Hz** | below this: feel degrades |
| Halo 3 host upstream breakdown | 50% kinematics / 20% input / 20% weapon events / 10% other | for comparison to our 25 Hz budget |

These bracket my earlier 512 kbps / connection recommendation: Halo's 250 kbps *total* for 16 clients works out to ~15 kbps per client when amortized. Our 512 kbps default is ~30× that — very safe. On the other side, Halo's 1 kbit/s per biped per client at combat quality is a useful floor: our 25 Hz × MTU bandwidth capacity easily supports tens of thousands of tile entities at replication rates far below combat-quality bipeds.

**The combined Halo lesson:** accumulators are necessary but not sufficient. The priority *function*, the traffic *tiering*, and the profiling *tooling* are all first-class features. A priority accumulator shipped without a way to inspect its decisions is a shipping bug waiting to happen (see: idle grenades).

### Unreal Engine — `IsNetReady()` collapse (Streeting, 2025)

Unreal's replication layer has the complete set — priority, relevancy, bandwidth caps — and still exhibits a pathology structurally identical to Naia's Phase 4.5 spike. Bulk reliable RPCs push `QueuedBits` past a threshold; `IsNetReady()` then **aborts the entire replication tick** for 45 + seconds. Fix required raising bandwidth caps to Fortnite-grade values and manually gating sends on `IsNetReady()`.

**Lesson for Naia:** a bandwidth cap without a priority lane across channel types can itself become the starvation mechanism. The priority + bandwidth design must allow mixed-channel fairness, not "reliable drains first, state starves."

### bevy_replicon — issue #57

A Rust Bevy networking crate in the same design space as Naia. Open proposal to add priority accumulators to replicon, citing Fiedler directly. Unresolved open questions in that issue map one-to-one to ours:

- per-packet vs per-entity acks
- spawn-burst handling
- whether "always send spawns every tick" is a special-cased API or an emergent property of a high initial accumulator value

We should not duplicate their unresolved questions — we should pick answers.

### Unity Netcode for Entities

Ships a per-tick snapshot budget clamped to MTU with priority-sorted ghost-chunk inclusion. Production-tested at scale. Structurally: same as Fiedler's state-sync accumulator, extended with ghost-chunk grouping.

### Valve — GameNetworkingSockets

Implements reliable/unreliable over UDP with a per-connection send queue and scheduler. No public writeup on priority accumulator semantics but the scheduling surface is structurally equivalent.

---

## Why Naia needs this — concrete mapping

### Current send path (verified, 2026-04-24)

The machinery below lives in `shared/` and is used identically by both server and client send loops; the spike we measured is on the server side only because our bench matrix exercises server→client traffic, but the same pathology is available in the other direction for any client-authoritative flow at comparable volume.

1. `ReliableSender::collect_messages(now, rtt_millis)` — pushes all messages whose `last_sent + 1.5 × rtt_ms ≤ now` into `outgoing_messages` (a `VecDeque`). All-or-nothing: an entire resend window fires on a single tick.
2. `IndexedMessageWriter::write_messages` — drains `outgoing_messages` into the *current* packet's `BitWriter` until overflow, then stops. Unwritten messages stay in the queue.
3. `Connection::send_packets` loops `send_packet` **until the queue is empty**. There is **no bandwidth cap, no per-tick packet cap, and no priority**. Server-side code: `server/src/connection/connection.rs:263-282` — `loop { if self.send_packet(…) { any_sent = true; } else { break; } }`. The client-side connection runs a structurally equivalent loop.

### The Phase 4.5 spike, narrated in this vocabulary

- At t = 0: 10 K entity-spawn commands queued into `UnorderedReliable` (`sending_messages` grows to 10 000).
- Server writes them. MTU ≈ 1200 bytes ≈ ~100 messages/packet → 100 packets sent in tick t=0. Fine at t=0 because we haven't hit the resend window yet.
- Between t=0 and the RTT-factor window (`1.5 × rtt_ms ≈ 850 ms ≈ 17 ticks at 50 ms/tick`), no resends trigger; baseline idle is 30 µs.
- At t≈17: every one of the 10 000 messages becomes due simultaneously (`last_sent + 1.5 × rtt_ms ≤ now`). `collect_messages` pushes all 10 000 into `outgoing_messages`. `send_packets` then loops **100 times in one tick**, each iteration building and emitting a full MTU-sized packet.
- This is the ~80 ms spike seen at `16u_10000e_mut` (100 packets × per-packet write cost, fanned out across connections on the sender).
- Next trigger is ~17 ticks later. Cadence matches observation exactly.

### What a bandwidth accumulator does to this timeline

Suppose we set `target_bytes_per_sec = 64 000` per connection (512 kbps, unremarkable). At 50 ms/tick: `budget += 3 200` bytes/tick. One packet ≈ 1200 bytes → 2 to 3 packets/tick/connection max.

- At t≈17: `collect_messages` still re-queues 10 000 messages into `outgoing_messages`. That part is unchanged.
- `send_packets` sends up to 2–3 packets/tick (budget limit). `send_packet` returns false once budget is exhausted. Remaining 97 packets' worth of messages stay queued in `outgoing_messages`.
- Next tick: budget resets (+3 200 bytes), another 2–3 packets flush.
- Total drain: ~100 packets ÷ 3 packets/tick ≈ **33 ticks ≈ 1.65 s of steady 50 ms/tick work**. Max tick cost drops from ~80 ms to ~2.4 ms (2–3 × per-packet cost).
- No spike. At the per-tick level, *everything* is smooth. Observable bandwidth is identical; latency distribution is transformed.

The *caveat*: with the bandwidth cap, each message is re-marked as "sent" by `collect_messages` only when it actually writes to a packet. The fast-path from Phase 4 handles this correctly — `min_last_sent` only moves forward on actual writes, so nothing gets starved.

### What a priority accumulator adds on top

The bandwidth accumulator alone gives us pacing — but picks work by **position in the VecDeque** (FIFO). For reliable messages that's fine; delivery order is the spec. For component updates it's wrong: a stationary stone that mutated once should yield to a unit that mutates every tick.

The priority accumulator turns "spend budget B" into "spend budget B on the top-B items by priority." For Naia's surfaces:

- **Component updates:** priority = f(staleness, scope distance, is-player-controlled, recently-mutated). This subsumes "dirty set" selection: items with no gain contribute no priority and never rise.
- **Entity commands / reliable messages:** priority ≈ insertion-age or rtt-stale-ness — mostly FIFO but we can boost responses (which unblock clients).
- **Plain messages:** per-channel priority is sufficient; no per-message sort.
- **Request / response:** responses carry an implicit priority bump (client is blocked on them).

---

## Explicit answers to the sidequest's research questions

### A. Fiedler's concept — definitive synthesis

- **Priority accumulator:** a per-item persistent float, incremented by a priority function each tick, used to sort-select items into packet space; reset on send, retained on skip. Scope in Fiedler's original writeup: state updates only. Prevents starvation, self-adapts to bandwidth changes.
- **Bandwidth accumulator:** a per-connection persistent scalar budget, incremented by `bytes_per_sec × dt` each tick, decremented per send. Scope in Fiedler's original writeup: large-block fragment pacing. Generic token-bucket.
- **Edge cases called out in primary sources:** starvation (solved structurally), priority inversion ("red katamari objects starve white ones" — also solved structurally). No warnings given about oscillation or accumulator overflow for the priority variant.

### B. Naia-surface applicability

| Surface                              | Bandwidth accum? | Priority accum?   | Notes |
|--------------------------------------|------------------|-------------------|-------|
| Component updates (state)            | YES (critical)    | YES (high value)   | Classic Fiedler scope. |
| `UnorderedReliable` entity commands  | YES (critical)    | optional          | Phase 4.5 directly depends on this. FIFO is fine for priority. |
| `OrderedReliable` entity commands    | YES (critical)    | NO (order is spec) | Must respect protocol order. |
| Plain messages                       | YES               | per-channel       | Low-volume usually; budget mostly protective. |
| Request / response                   | YES               | priority bump     | Responses > requests > state. |
| Unreliable channels (`Sequenced`)    | YES               | optional          | Already self-paced by update frequency, but budget is a global safety net. |

### C. Interactions with existing machinery

- **RTT-based resend (`ReliableSender::collect_messages`)**: unchanged in logic; the accumulator acts downstream at `send_packets`. The Phase 4 fast-path continues to apply.
- **Dirty set (Phase 3)**: dirty items contribute priority; the priority function reads dirty state. Dirty set remains the O(dirty) index; accumulator sorts within it.
- **MTU / framing**: bandwidth accumulator respects MTU per packet (unchanged); it just decides *how many packets* to emit per tick.
- **Protocol wire format**: **no changes required**. Accumulators are purely **sender-side** decisions and apply symmetrically to server→client and client→server flows. Naia supports client-authoritative messages, requests, responses, and entities, so the accumulator lives in whichever side is assembling outbound packets. This is a hard scope guarantee.

### D. Does this fix Phase 4.5?

**Yes, with high confidence.** The spike is a pure "too many packets in one tick" pathology; the bandwidth accumulator explicitly and correctly bounds packets-per-tick. Re-running `idle_distribution.rs` with a modest budget should collapse the max/p50 ratio from ~3000× to < 10× on every mutable cell.

The priority accumulator layer is not required to fix the spike; it is required to make component-update fan-out scale to 64+ players × 65 K tiles. Connor's direction on whether to ship both together or bandwidth-first / priority-second is an open question below.

### E. Guardrails & failure modes

- **Starvation:** solved by accumulator retention for skipped items. Monitor: p99.9 age-of-oldest-unsent-message.
- **Priority function tuning — the idle-grenade trap** (Halo: Reach, 2011): a priority function that inherits boosts across categories can end up spending bandwidth on stationary junk. Mitigate by (a) shipping a sane default per channel kind that doesn't boost by inheritance, (b) inspectable runtime API — "why is item X at priority Y?", (c) a priority histogram in `bench_instrumentation` so pathological tails are visible without a bug report.
- **Budget set too low:** latency climbs, acks bunch up. Mitigate by bandwidth-monitor telemetry (already partially present via `bandwidth_monitor.rs`) + a reasonable default that's well above observed steady-state.
- **Budget set too high:** degenerates to current behavior, no regression risk. Safe default.
- **Mixing reliable + skippable under one priority:** reliable messages can't be dropped by priority (only paced by budget), so a naive "top-K by priority" selector that pops reliables off would break delivery guarantees. Mitigate by two-phase send: bandwidth-reserve for reliables first, priority-select over skippables with the remainder. This matches Halo's "unreliability enables aggressive prioritization" framing.

### F. Tests / invariants

- **Eventual delivery:** every reliable message sent is eventually acked, under any non-adversarial budget. Namako spec.
- **Budget respected:** per-tick bytes-sent ≤ budget + one-packet slack. Unit test on the accumulator.
- **Priority ordering observable:** higher-priority item sent first in a mixed queue when budget is tight. Unit test.
- **No regression on `idle_distribution`:** every cell `OK` (max/p50 ≤ 10×).
- **No regression on criterion matrix:** all `tick/idle_matrix` cells hold Phase 4 numbers or better.
- **Namako gate green:** all 22 feature specs.

---

## Open decisions for Connor (needed before the implementation plan)

These are the judgment calls. The research has scoped them; Connor picks the path.

1. **Sequencing.** Which of the following?
   - **(a) bandwidth-first:** ship bandwidth accumulator only (resolves Phase 4.5). Add priority accumulator as a separate follow-up.
   - **(b) combined:** ship bandwidth + priority together as the full sidequest.
   - **(c) priority-first:** controversial; would not resolve Phase 4.5 by itself.
   - **Claude's recommendation: (a), then (b) as a natural continuation.** Lowers risk, unblocks Phase 5 sooner, and makes priority a cleanly-layered addition later.

2. **Budget target default.** What's the per-connection `target_bytes_per_sec`?
   - **Claude's recommendation:** 64 000 B/s (512 kbps) as the default, configurable. Matches Fiedler's example bracket; generous headroom for games; still bounds bursts meaningfully.

3. **Global budget vs per-channel budget.** One connection-wide budget, or one per channel?
   - **Claude's recommendation:** single connection-wide budget + per-channel `fair_share` weights summing to 1.0. Keeps the accounting simple while preventing any one channel (e.g., `UnorderedReliable` spawn burst) from starving another (e.g., `OrderedReliable` gameplay events). Defaults produce behavior indistinguishable from "round-robin across channels with data."

4. **Where does the accumulator live?** In each peer's outbound-packet assembly — **sender-side, symmetric for server and client**. Concretely: wrap the `send_packets` loop on `server::connection::Connection` *and* the equivalent loop on the client-side connection. Accumulator logic is identical on both sides; only the budget target and telemetry surface may differ.
   - **Claude's recommendation:** a shared implementation in `shared/src/connection/` (new module) consumed by both `server::connection::Connection::send_packets` and `client::connection::Connection::send_packets`. Channels stay unchanged APIs; only the enclosing loop gains budget accounting. This keeps surfaces minimal, preserves Phase 4's invariants, and applies cleanly to client-authoritative flows (client-owned entities, client-issued messages/requests/responses).

5. **Priority function shape (for step 2 later).** User-supplied callback per channel? Built-in heuristic with hooks? Declarative per-component annotation?
   - **Claude's recommendation:** built-in heuristic per channel kind, with an opt-in trait callback for custom priority. Ship sane defaults so 95% of users never think about it.

6. **Telemetry.** What counters under `bench_instrumentation`?
   - Bytes sent / tick / channel.
   - Budget remaining at end of tick.
   - Oldest unsent item age / channel.
   - Items deferred due to budget / tick.
   - **Decision needed:** any of these promoted to always-on observability (outside `bench_instrumentation`)?

7. **Phase 4.5 fate.** If sequencing (a) is chosen: after the bandwidth accumulator lands, re-run `idle_distribution` and confirm `OK` across all mutable cells. If confirmed, Phase 4.5 is closed by absorption (no separate fix, no separate log). If the spike persists, 4.5 becomes a fresh root-cause hunt against the post-accumulator baseline.

---

## Recommended next artifact

Once Connor reviews and picks the sequencing / defaults, I'll write `_AGENTS/PRIORITY_ACCUMULATOR_PLAN.md` with:

- concrete phase structure (A: bandwidth; B: priority)
- files-touched list per phase
- test strategy per phase
- criterion + `idle_distribution` gates per phase
- risk register with rollback conditions

No code lands until that plan is approved.

---

## Sources

- Glenn Fiedler — [State Synchronization (Gaffer On Games, 2015)](https://gafferongames.com/post/state_synchronization/) — priority accumulator proper.
- Glenn Fiedler — [Sending Large Blocks of Data (Gaffer On Games, 2016)](https://gafferongames.com/post/sending_large_blocks_of_data/) — bandwidth accumulator.
- Glenn Fiedler — [Reliable Ordered Messages (Gaffer On Games, 2016)](https://gafferongames.com/post/reliable_ordered_messages/) — counterpoint algorithm (no accumulator).
- Glenn Fiedler — [Building a Game Network Protocol series](https://gafferongames.com/categories/building-a-game-network-protocol/) — context for the reliability model.
- [gafferongames/gafferongames state_synchronization.md (raw)](https://github.com/gafferongames/gafferongames/blob/master/content/post/state_synchronization.md) — primary source, verbatim.
- Steve Streeting — [Problems With Unreal Network Saturation (2025)](https://www.stevestreeting.com/2025/05/12/problems-with-unreal-network-saturation/) — production-scale failure mode in Unreal that maps to Naia's Phase 4.5 spike.
- [bevy_replicon issue #57](https://github.com/simgine/bevy_replicon/issues/57) — parallel proposal in a sibling Rust crate.
- Unity — [Netcode for Entities ghost snapshots](https://docs.unity3d.com/Packages/com.unity.netcode@1.5/manual/ghost-snapshots.html) — shipping priority + budget design.
- Unreal — [Actor Priority in Unreal Engine (Epic Docs)](https://dev.epicgames.com/documentation/en-us/unreal-engine/actor-priority-in-unreal-engine) — shipping priority model (for comparison).
- [Valve GameNetworkingSockets](https://github.com/ValveSoftware/GameNetworkingSockets) — structurally adjacent protocol, referenced for scheduling surface.
- David Aldridge & Patrick Parsons (Bungie) — *I Shot You First: Networking the Gameplay of Halo: Reach* ([GDC Vault](https://www.gdcvault.com/play/1014345/I-Shot-You-First-Networking) · [YouTube](https://www.youtube.com/watch?v=h47zZrqjgLc) · [slide summary](http://slidegur.com/doc/219604/gameplay-networking-of-halo--reach)) — shipping AAA netcode, per-receiver per-object priority, unreliability-enables-prioritization, idle-grenade bug, network-profiler-in-replays tooling, concrete kbps / Hz targets.
- [Wolfire Games blog — GDC session summary](http://blog.wolfire.com/2011/03/GDC-Session-Summary-Halo-networking) — secondary distillation of the above.
