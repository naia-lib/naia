// Phase 4 diagnostic: time each sub-phase of an idle tick to find where the
// remaining ~1 ms at 16u_10000e immutable actually lives. Phase 3 proved the
// dirty scan is gone; this probe breaks tick() into outer buckets AND drills
// into `server.send_all_packets` via the `bench_instrumentation`-gated
// `bench_send_counters` on `naia::connection::Connection::send_packets`.
//
// Outer buckets:
//   hub:     hub.process_time_queues
//   clients: per-client receive/process/send loop
//   srv_rx:  server.receive_all_packets + process_all_packets
//   srv_tx:  server.send_all_packets   ← the bulk of idle cost
//   drain:   drain_all_events
//
// Inner (per-user, aggregated across U): ns spent in
//   collect_msgs: base.collect_messages (heartbeat, ping, reliable resends)
//   take_events:  world_manager.take_outgoing_events (builds dirty map)
//   send_loop:    the packet-writing loop
//
// Run with:
//   cargo run --release --example phase4_tick_internals -p naia-benches

use naia_benches::BenchWorldBuilder;
use naia_server::bench_send_counters;
use naia_shared::bench_take_events_counters;

fn run(u: usize, n: usize, immutable: bool) {
    let mut builder = BenchWorldBuilder::new().users(u).entities(n);
    if immutable {
        builder = builder.immutable();
    }
    let mut world = builder.build();

    const TICKS: usize = 20;
    const WARMUP: usize = 5;
    let mut hub = Vec::with_capacity(TICKS);
    let mut clients = Vec::with_capacity(TICKS);
    let mut srv_rx = Vec::with_capacity(TICKS);
    let mut srv_tx = Vec::with_capacity(TICKS);
    let mut drain = Vec::with_capacity(TICKS);
    let mut collect_msgs = Vec::with_capacity(TICKS);
    let mut take_events = Vec::with_capacity(TICKS);
    let mut send_loop = Vec::with_capacity(TICKS);
    let mut hrc = Vec::with_capacity(TICKS);
    let mut scoll = Vec::with_capacity(TICKS);
    let mut tue = Vec::with_capacity(TICKS);

    for _ in 0..TICKS {
        bench_send_counters::reset();
        let b = world.tick_timed();
        let (c_ns, t_ns, s_ns) = bench_send_counters::snapshot();
        let (h_ns, sc_ns, tu_ns) = bench_take_events_counters::snapshot();
        hub.push(b.hub.as_secs_f64() * 1e6);
        clients.push(b.clients.as_secs_f64() * 1e6);
        srv_rx.push(b.srv_rx.as_secs_f64() * 1e6);
        srv_tx.push(b.srv_tx.as_secs_f64() * 1e6);
        drain.push(b.drain.as_secs_f64() * 1e6);
        collect_msgs.push(c_ns as f64 / 1e3);
        take_events.push(t_ns as f64 / 1e3);
        send_loop.push(s_ns as f64 / 1e3);
        hrc.push(h_ns as f64 / 1e3);
        scoll.push(sc_ns as f64 / 1e3);
        tue.push(tu_ns as f64 / 1e3);
    }

    let median = |v: &mut Vec<f64>| -> f64 {
        v.drain(..WARMUP);
        v.sort_by(|x, y| x.partial_cmp(y).unwrap());
        v[v.len() / 2]
    };
    let hub_m = median(&mut hub);
    let cli_m = median(&mut clients);
    let rx_m = median(&mut srv_rx);
    let tx_m = median(&mut srv_tx);
    let dr_m = median(&mut drain);
    let cm_m = median(&mut collect_msgs);
    let te_m = median(&mut take_events);
    let sl_m = median(&mut send_loop);
    let hrc_m = median(&mut hrc);
    let scoll_m = median(&mut scoll);
    let tue_m = median(&mut tue);
    let total = hub_m + cli_m + rx_m + tx_m + dr_m;
    let label = if immutable { "imm" } else { "mut" };
    println!(
        "U={u:>2} N={n:>5} {label} | total={total:>7.1}µs  clients={cli_m:>7.1}  srv_tx={tx_m:>7.1}  drain={dr_m:>5.1}",
    );
    println!(
        "           |   send_packets: collect_msgs={cm_m:>6.1}µs  take_events={te_m:>6.1}µs  send_loop={sl_m:>6.1}µs",
    );
    println!(
        "           |   take_events: host/remote_cmds={hrc_m:>6.1}µs  sender_collect={scoll_m:>6.1}µs  update_events={tue_m:>6.1}µs  residual={:>6.1}µs",
        te_m - hrc_m - scoll_m - tue_m,
    );
}

fn main() {
    println!("=== Phase 4 diagnostic: idle tick internals by (U, N) — microseconds ===");
    println!();
    for (u, n) in [(1, 10_000), (4, 10_000), (16, 10_000)] {
        run(u, n, false);
    }
    println!();
    println!("=== Immutable ===");
    println!();
    for (u, n) in [(1, 10_000), (4, 10_000), (16, 10_000)] {
        run(u, n, true);
    }
}
