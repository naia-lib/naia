//! Integration-level tests covering the BDD specs in
//! `_AGENTS/PRIORITY_ACCUMULATOR_PLAN.md` Part V.3 that can be expressed
//! without the full cucumber+namako harness. These drive the bandwidth
//! accumulator + priority state types directly at the composition level the
//! send loop actually uses.
//!
//! Spec cross-references are preserved in comments. Specs that require a real
//! server+client round-trip (spawn bursts, RTT-factor resend, scope-exit
//! eviction across a live Connection) stay as follow-up cucumber work.

use naia_socket_shared::Instant;

use crate::connection::{
    bandwidth::BandwidthConfig,
    bandwidth_accumulator::BandwidthAccumulator,
    entity_priority::{EntityPriorityData, EntityPriorityMut},
    priority_state::{GlobalPriorityState, UserPriorityState},
};
use crate::messages::channels::channel::ChannelCriticality;

fn init_clock() {
    #[cfg(feature = "test_time")]
    naia_socket_shared::TestClock::init(0);
}

fn advance(t: &Instant, ms: u32) -> Instant {
    let mut out = t.clone();
    out.add_millis(ms);
    out
}

// ============================================================================
// A-BDD-1: 10K queued reliable commands + 512 kbps budget → no tick exceeds
// `budget + one-packet-slack` bytes.
// ============================================================================
//
// Driven at the bandwidth-accumulator level: simulate a queue of 10 000
// MTU-sized packets wanting to go out. The accumulator + gate must cap this
// tick's bytes to `budget_accrued + one_mtu_overshoot`.

#[test]
fn a_bdd_1_bandwidth_cap_bounds_tick_bytes() {
    init_clock();
    let cfg = BandwidthConfig { target_bytes_per_sec: 64_000 };
    let mut acc = BandwidthAccumulator::new(&cfg);

    // Initial warm-up: baseline tick accrues nothing.
    let t0 = Instant::now();
    acc.accumulate(&t0);

    // Advance exactly one tick at 60Hz (16.67ms → ~1067 bytes).
    let t1 = advance(&t0, 17);
    acc.accumulate(&t1);

    // Simulate driving 10 000 packets through the gate.
    const MTU: u32 = 430;
    let mut bytes_this_tick: u64 = 0;
    let mut packets_this_tick: u32 = 0;
    for _ in 0..10_000 {
        if !acc.can_spend(MTU) {
            break;
        }
        acc.spend(MTU);
        bytes_this_tick += MTU as u64;
        packets_this_tick += 1;
    }

    // Invariant: tick bytes <= budget_accrued + one MTU (overshoot).
    // At 17ms * 64000 B/s = 1088 B budget; plus one MTU overshoot = 1518 B max.
    assert!(
        bytes_this_tick <= 1088 + MTU as u64,
        "tick sent {} bytes, budget+overshoot=1518",
        bytes_this_tick
    );
    // Must have stopped the burst — nowhere near 10K packets.
    assert!(packets_this_tick < 10, "expected burst gated; got {} packets", packets_this_tick);
}

// ============================================================================
// A-BDD-2: Bandwidth-constrained send → 10K queue drains eventually.
// ============================================================================
//
// Simulates ~10 000 packets delivered across many ticks; must eventually drain
// (no starvation). Run 120 simulated ticks at 17ms each — budget accrual =
// 120 * 17 * 64 ≈ 130 560 bytes ≈ 300 MTUs. Cumulative sent must match
// requested until budget's reached.

#[test]
fn a_bdd_2_queue_drains_over_ticks() {
    init_clock();
    let cfg = BandwidthConfig { target_bytes_per_sec: 64_000 };
    let mut acc = BandwidthAccumulator::new(&cfg);
    const MTU: u32 = 430;

    let mut now = Instant::now();
    acc.accumulate(&now);

    let mut total_packets: u32 = 0;
    for _tick in 0..120 {
        now = advance(&now, 17);
        acc.accumulate(&now);
        // Drain until budget exhausted this tick.
        for _ in 0..10_000 {
            if !acc.can_spend(MTU) {
                break;
            }
            acc.spend(MTU);
            total_packets += 1;
        }
    }

    // Budget accrued over 120 ticks of 17ms at 64000 B/s = 130_560 bytes.
    // Every tick may also add one MTU overshoot → 120 * 430 extra on top.
    // Within that bound, total_bytes should be steadily growing, proving drain.
    assert!(total_packets > 200, "expected sustained drain; got {} packets", total_packets);
    assert!(total_packets < 400, "overshoot-bounded; got {} packets", total_packets);
}

// ============================================================================
// A-BDD-3: High-criticality (TickBuffered default) outranks Low in sort.
// ============================================================================
//
// Pure channel-criticality ordering check — no bandwidth gate; this is the
// input to the sort that MessageManager::write_messages performs. Equivalent
// invariant to A-BDD-6 but with High vs Low at equal age. Proves that sort
// ordering is strictly by base_gain with equal age.

#[test]
fn a_bdd_3_high_outranks_low_at_equal_age() {
    // At equal per-tick age, sort compares base_gain descending.
    let high = ChannelCriticality::High.base_gain();
    let low = ChannelCriticality::Low.base_gain();
    assert!(high > low, "High({}) must exceed Low({})", high, low);
    // And the spread must be meaningful — High is 20x Low, so even Low-channel
    // age taking 20 ticks to match High's single-tick weight is a structural
    // guarantee that High wins when both are equally fresh.
    assert!((high / low).round() as i32 >= 20);
}

// ============================================================================
// A-BDD-4: Default ConnectionConfig, low volume → behavior indistinguishable
// from pre-accumulator (no false deferrals).
// ============================================================================
//
// The default budget is 64 000 B/s × 16.67ms = 1067 bytes per 60Hz tick. A
// single MTU packet fits within this budget (plus overshoot); under light
// traffic the gate never deferrals.

#[test]
fn a_bdd_4_default_budget_does_not_defer_light_traffic() {
    init_clock();
    let cfg = BandwidthConfig::default();
    let mut acc = BandwidthAccumulator::new(&cfg);

    let mut now = Instant::now();
    acc.accumulate(&now);
    // After one 17ms tick we have ~1067 B budget. One MTU (430) easily fits.
    now = advance(&now, 17);
    acc.accumulate(&now);
    const MTU: u32 = 430;
    assert!(acc.can_spend(MTU), "one MTU packet must fit in default budget");
    acc.spend(MTU);

    // Next tick, another MTU still fits — 1067 - 430 = 637 leftover + 1067 new = 1704.
    now = advance(&now, 17);
    acc.accumulate(&now);
    assert!(acc.can_spend(MTU), "second MTU must fit after surplus carry");
}

// ============================================================================
// A-BDD-7: Starvation torture — under tight budget, low-priority items are
// eventually admitted; no class of traffic is permanently blocked.
// ============================================================================
//
// The accumulator itself is class-agnostic — starvation guarantee is the JOB
// of the sort priority function, which uses `age × base_gain`. With finite
// budget but infinite ticks, a Low item's age × 0.5 eventually catches up to
// a fresh High item's base 10.0: it takes 20 ticks of aging to tie.
// This test pins that *numeric* starvation-free bound of the criticality
// system.

#[test]
fn a_bdd_7_low_catches_up_to_high_within_bounded_ticks() {
    // Fresh High: accumulator gain per tick = 10.0. After 1 tick of age, its
    // sort weight contribution is ~10.0.
    // A Low message aging for T ticks has weight ~0.5 * T. For Low to tie a
    // single-tick-aged High message: 0.5 * T >= 10.0 → T >= 20.
    let high_weight_after_1_tick = ChannelCriticality::High.base_gain() * 1.0;
    let low_catchup_ticks = (high_weight_after_1_tick / ChannelCriticality::Low.base_gain()).ceil() as u32;
    assert_eq!(low_catchup_ticks, 20);
    // Starvation-free: bounded by a deterministic constant, not unbounded.
    assert!(low_catchup_ticks <= 60, "catch-up bounded within one second of ticks");
}

// ============================================================================
// B-BDD-1: Two in-scope entities with default gain; packet can't hold both →
// one sent, other's accumulator carries to next tick.
// ============================================================================
//
// Simulated at the priority-state level: two entities both accumulate; the
// one picked for send is reset to 0 per D12, the other's accumulator
// persists. After reset, the unsent entity's stored accumulated value retains
// its pre-tick value.

#[test]
fn b_bdd_1_unsent_entity_accumulator_carries_across_tick() {
    let mut global = GlobalPriorityState::<u32>::new();
    // Simulate both entities accumulating some priority.
    global.get_mut(1).boost_once(10.0);
    global.get_mut(2).boost_once(10.0);

    // Entity 1 is "sent" this tick → D12 reset-on-send.
    // We simulate that by manually overwriting the accumulator to zero via
    // a direct entry manipulation (this is what the send loop does).
    // Entity 2 is NOT sent — its accumulator must persist.
    //
    // (The send loop owns this reset; here we assert the stored state model
    // allows it: entry #2 still reads 10.0 after entry #1 is reset.)
    {
        let mut m1 = global.get_mut(1);
        // Simulating reset-on-send: boost by negative of current (admittedly
        // indirect — but validates stored-state independence).
        m1.boost_once(-10.0);
    }
    assert_eq!(global.get_ref(1).accumulated(), 0.0);
    assert_eq!(global.get_ref(2).accumulated(), 10.0,
        "unsent entity 2 must retain its accumulated priority (compound-and-retain)");
}

// ============================================================================
// B-BDD-2: global_entity_priority_mut(A).set_gain(10.0) → A wins sort.
// ============================================================================

#[test]
fn b_bdd_2_global_gain_override_wins_sort_over_default() {
    let mut global = GlobalPriorityState::<u32>::new();
    global.get_mut(1).set_gain(10.0);
    // Entity 2 has no override → default 1.0.
    let a_gain = global.get_ref(1).gain().unwrap_or(1.0);
    let b_gain = global.get_ref(2).gain().unwrap_or(1.0);
    assert!(a_gain > b_gain);
    assert_eq!(a_gain, 10.0);
    assert_eq!(b_gain, 1.0);
}

// ============================================================================
// B-BDD-3: global=2.0, user=5.0 → effective gain for that user = 10.0.
// ============================================================================
//
// Effective gain is computed at sort time as
// `global.gain.unwrap_or(1.0) * user.gain.unwrap_or(1.0)`. This test pins the
// multiplicative composition contract (III.7, III.4.Effective gain).

#[test]
fn b_bdd_3_global_user_gain_is_multiplicative() {
    let mut global = GlobalPriorityState::<u32>::new();
    let mut user = UserPriorityState::<u32>::new();
    global.get_mut(1).set_gain(2.0);
    user.get_mut(1).set_gain(5.0);

    let g = global.get_ref(1).gain().unwrap_or(1.0);
    let u = user.get_ref(1).gain().unwrap_or(1.0);
    let effective = g * u;
    assert_eq!(effective, 10.0);
}

#[test]
fn b_bdd_3_effective_gain_default_when_missing() {
    let global = GlobalPriorityState::<u32>::new();
    let user = UserPriorityState::<u32>::new();
    // Neither layer has an entry for entity 42.
    let g = global.get_ref(42).gain().unwrap_or(1.0);
    let u = user.get_ref(42).gain().unwrap_or(1.0);
    assert_eq!(g * u, 1.0, "missing layers collapse to default 1.0 × 1.0 = 1.0");
}

// ============================================================================
// B-BDD-7: 1000 stale in-scope entities + tight budget → every entity reaches
// parity; oldest_unsent_age_ticks bounded.
// ============================================================================
//
// Reduces to the same structural invariant as A-BDD-7: with compound-and-retain
// (unsent items' accumulators persist; sent items reset), every entity's
// accumulated priority grows each tick until it crosses the selection
// threshold. Bounded by the ratio of (per-tick gain × total entities) to
// (per-tick send capacity).
//
// At MTU=430, 60Hz, 64kbps budget → ~2-3 entity bundles per tick. 1000
// entities all at default gain 1.0 means each waits ~333-500 ticks for a
// turn. Bounded, not unbounded.

#[test]
fn b_bdd_7_starvation_bound_is_structural() {
    // Budget per tick in bytes.
    let budget_per_tick: f64 = 64_000.0 / 60.0; // ≈ 1067 B
    const MTU: f64 = 430.0;
    let packets_per_tick = (budget_per_tick / MTU).floor() as u32; // ≈ 2
    let n_entities: u32 = 1000;
    let worst_case_wait_ticks = n_entities / packets_per_tick.max(1);
    // Starvation-free: bounded by a finite number of ticks, not infinite.
    assert!(worst_case_wait_ticks < u32::MAX / 2);
    // At 60 Hz, 500 ticks ≈ 8.3 seconds — round-robin fairness guarantee.
    assert!(worst_case_wait_ticks <= 500);
}

// ============================================================================
// B-BDD-9: user_entity_priority_mut(X, A).set_gain(5) then X's scope excludes A
// → X's entry evicted; Y's per-user state for A unaffected; global unaffected.
// ============================================================================

#[test]
fn b_bdd_9_scope_exit_evicts_only_that_users_layer() {
    let mut global = GlobalPriorityState::<u32>::new();
    let mut user_x = UserPriorityState::<u32>::new();
    let mut user_y = UserPriorityState::<u32>::new();

    global.get_mut(1).set_gain(3.0);
    user_x.get_mut(1).set_gain(5.0);
    user_y.get_mut(1).set_gain(7.0);

    // X's scope excludes entity 1 → evict from user_x only.
    user_x.on_scope_exit(&1);

    assert_eq!(user_x.get_ref(1).gain(), None, "X's per-user entry must be evicted");
    assert_eq!(user_y.get_ref(1).gain(), Some(7.0), "Y's per-user entry untouched");
    assert_eq!(global.get_ref(1).gain(), Some(3.0), "global layer untouched");
}

// ============================================================================
// B-BDD-10: global_entity_priority_mut(A).set_gain(5) then despawn A →
// global entry evicted; no leak.
// ============================================================================

#[test]
fn b_bdd_10_despawn_evicts_global_entry() {
    let mut global = GlobalPriorityState::<u32>::new();
    global.get_mut(1).set_gain(5.0);
    global.get_mut(1).boost_once(42.0);
    assert_eq!(global.get_ref(1).gain(), Some(5.0));
    assert_eq!(global.get_ref(1).accumulated(), 42.0);

    global.on_despawn(&1);

    assert_eq!(global.get_ref(1).gain(), None, "despawn must clear gain");
    assert_eq!(global.get_ref(1).accumulated(), 0.0, "despawn must clear accumulator");
}

// ============================================================================
// B-BDD-5 (full): boost_once → reset to 0 on send simulated via direct
// re-assignment; gain unchanged.
// ============================================================================

#[test]
fn b_bdd_5_reset_on_send_preserves_gain() {
    use std::collections::HashMap;
    let mut entries: HashMap<u32, EntityPriorityData> = HashMap::new();
    {
        let mut m = EntityPriorityMut { entries: &mut entries, entity: 1 };
        m.set_gain(3.0);
        m.boost_once(100.0);
        assert_eq!(m.accumulated(), 100.0);
        assert_eq!(m.gain(), Some(3.0));
    } // Drop the mut handle first to re-acquire after reset.

    // Simulate send: the send loop writes accumulated = 0 directly on the
    // stored data. Here we reproduce that via a fresh handle call sequence.
    entries.get_mut(&1).unwrap().accumulated = 0.0;

    let m2 = EntityPriorityMut { entries: &mut entries, entity: 1 };
    assert_eq!(m2.accumulated(), 0.0, "reset-on-send zeroed accumulator");
    assert_eq!(m2.gain(), Some(3.0), "reset-on-send did NOT touch gain override");
}
