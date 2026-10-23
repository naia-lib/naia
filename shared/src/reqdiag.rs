//! TEMP RED 108734 diag (Usher follow-up to 109849, branch `temp-reqdiag`
//! at ad00ad34c). Per-process request-path counters on the exact path of
//! Walt's publish request, printed as one summary line every 200 ticks per
//! side. NOT FOR MERGE — delete this file and all its hooks before any `dev`
//! landing.
//!
//! Counters (Usher 110231):
//! - `cli_queued`: client `send_request` accepted into the channel queue.
//! - `req_written`: request envelopes serialized into an outgoing packet.
//! - `req_acked`: a tracked request message index marked delivered by an ack.
//! - `msg_decoded`: request envelopes parsed out of an incoming packet.
//! - `chan_entered`: parsed request envelopes accepted by the reliable
//!   receiver buffer (`Buffered` / `ReplacedUpgrade`; duplicates and
//!   window-drops do NOT count).
//! - `surfaced`: requests inserted into the server world-events path
//!   (`push_request`, the feed of `RequestEvents`).
//!
//! The counter where the count goes 1 -> 0 across two adjacent stages names
//! the losing layer.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{LazyLock, Mutex};

static CLI_QUEUED: AtomicU64 = AtomicU64::new(0);
static REQ_WRITTEN: AtomicU64 = AtomicU64::new(0);
static REQ_ACKED: AtomicU64 = AtomicU64::new(0);
static MSG_DECODED: AtomicU64 = AtomicU64::new(0);
static CHAN_ENTERED: AtomicU64 = AtomicU64::new(0);
static SURFACED: AtomicU64 = AtomicU64::new(0);

/// Message indices of request envelopes written into packets, awaiting ack.
/// Only request indices are recorded (requests are rare; normal traffic never
/// enters this map), so u16 wrap-around cannot misattribute.
static UNACKED_REQ_INDEX: LazyLock<Mutex<HashMap<u16, ()>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

static CLI_PRINT_TICK: AtomicU64 = AtomicU64::new(0);
static SRV_PRINT_TICK: AtomicU64 = AtomicU64::new(0);

fn summary_line() -> String {
    format!(
        "cli_queued={} req_written={} req_acked={} msg_decoded={} chan_entered={} surfaced={}",
        CLI_QUEUED.load(Ordering::Relaxed),
        REQ_WRITTEN.load(Ordering::Relaxed),
        REQ_ACKED.load(Ordering::Relaxed),
        MSG_DECODED.load(Ordering::Relaxed),
        CHAN_ENTERED.load(Ordering::Relaxed),
        SURFACED.load(Ordering::Relaxed),
    )
}

pub fn bump_cli_queued() {
    CLI_QUEUED.fetch_add(1, Ordering::Relaxed);
}

pub fn note_req_written(message_index: u16) {
    REQ_WRITTEN.fetch_add(1, Ordering::Relaxed);
    if let Ok(mut map) = UNACKED_REQ_INDEX.lock() {
        map.insert(message_index, ());
    }
}

pub fn note_msg_delivered(message_index: u16) {
    let tracked = match UNACKED_REQ_INDEX.lock() {
        Ok(mut map) => map.remove(&message_index).is_some(),
        Err(_) => false,
    };
    if tracked {
        REQ_ACKED.fetch_add(1, Ordering::Relaxed);
    }
}

pub fn bump_msg_decoded() {
    MSG_DECODED.fetch_add(1, Ordering::Relaxed);
}

pub fn bump_chan_entered() {
    CHAN_ENTERED.fetch_add(1, Ordering::Relaxed);
}

pub fn bump_surfaced() {
    SURFACED.fetch_add(1, Ordering::Relaxed);
}

/// Emit the one-line summary every 200 client ticks.
pub fn maybe_print_cli() {
    if CLI_PRINT_TICK.fetch_add(1, Ordering::Relaxed) % 200 == 0 {
        eprintln!("[REQDIAG-clitick] {}", summary_line());
    }
}

/// Emit the one-line summary every 200 server ticks.
pub fn maybe_print_srv() {
    if SRV_PRINT_TICK.fetch_add(1, Ordering::Relaxed) % 200 == 0 {
        eprintln!("[REQDIAG-srvtick] {}", summary_line());
    }
}
