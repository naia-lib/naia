# Contract 21 — Pipeline Worker Handle Lease

## Context

`PipelineRuntime<E>` (`server/src/pipeline_actors/runtime.rs`) runs a recv
worker and a send worker beside the consumer's tick. Each worker drives one
pipeline handle (`RecvHandle<E>` / `SendHandle<E>`), and the consumer borrows
that same handle inside a *park window*: `park_workers()` ... consumer uses the
handle ... `unpark_workers()`.

Until nydus-y2e the handle lived in the shared slot (`Arc<Mutex<Option<H>>>`)
and the worker re-borrowed it every iteration with a `try_lock`, falling back to
a 100µs sleep on contention, and the idle send worker polled on a 100µs bounded
wait. Measured before the change (`cargo test -p naia-server --lib`,
`workers_active`, 300 ms idle):

| measure | before | after (DONE WHEN) |
|---|---|---|
| slot-lock acquisitions per receive iteration | 2 (2490 over 1245 iterations) | 0 |
| idle wakeups on the send worker | 1584 per 300 ms (about 5,280/s) | 0 |

This contract names the replacement: the worker **owns** its handle and
**lends** it through the park protocol, and the lending is carried by a type,
`HandleLease<H>`, not by a comment.

## Obligations

- **L-1 Ownership.** Outside a park window the worker holds its handle in its
  `HandleLease`; a receive or transmit iteration takes no lock on the slot.
  Test: `recv_worker_takes_no_slot_lock_per_iteration` (slot locks ≤ 2 over
  hundreds of iterations, the 2 being the first reclaim and the exit deposit).
- **L-2 Deposit before parked.** At the park checkpoint the worker deposits
  the handle into the slot **before** it increments `parked_count`, so when
  `park_workers()` returns every handle is in its slot. The consumer's
  `tick`, `take_handles`, `with_monolithic_world_server` and the stats readers
  rely on this and never wait for a handle.
- **L-3 Reclaim after resumed.** Leaving the checkpoint, an active
  (`workers_active`) worker reclaims the handle. If the consumer kept it
  (`take_handles` without `restore_handles`), the worker idles bounded (100µs)
  and retries; it never drops a buffered send job on a failed reclaim.
- **L-4 Deposit on exit, never on panic.** A worker that returns (shutdown,
  closed readiness, hung-up output channel) deposits a held handle through the
  lease's `Drop`, so the consumer's park-window borrow keeps working after the
  worker is gone. A worker that panics does not deposit: a handle interrupted
  mid-`receive()` or mid-transmit is not known to be sound, so the slot stays
  empty and the consumer's next borrow fails loud after
  `propagate_panic_if_any` surfaces the payload (unchanged from before).
- **L-5 Deterministic path never holds.** In `not(workers_active)` builds the
  lease never reclaims: the consumer drives recv/send synchronously inside the
  window and the workers are pure parking services.
- **W-1 Send worker wakes on a job, not a timer.** With nothing published and
  nothing buffered, the send worker sleeps unbounded on `WorkerWake`. It is
  woken only by `SnapshotSender::send`, `park_workers`, runtime `Drop`
  (shutdown) or the test-panic request. Test:
  `send_worker_idles_without_wakeups_and_wakes_on_publish` (0 idle wakeups
  over 300 ms, then a publish transmits within the deadline).
- **W-2 No lost wake.** `WorkerWake` evaluates the idle condition under its
  mutex and every producer sets its state before `notify()`, which takes that
  mutex. No bound is needed for correctness; the recv worker on a poll-only
  transport keeps a 100µs bound only because it must re-poll the socket.
- **W-3 One-tick lag preserved.** A freshly published job is still buffered
  for one iteration and transmitted on the next; the worker loops immediately
  when a job is buffered rather than sleeping.

## Removed / not wanted

- **Per-iteration `try_lock` + 100µs sleep claim/deposit loops** (removed by
  nydus-y2e, Nash, 2026-11-10): they cost two slot locks per receive iteration
  and a retry sleep on contention. The lease replaces them.
- **Bounded 100µs idle poll on the send worker** (removed by nydus-y2e): it
  produced about 5,280 wakeups per second with nothing to send. The publish
  now wakes the worker.
