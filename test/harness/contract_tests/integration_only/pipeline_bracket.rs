//! G7 — `PipelinedServer::{receive, send}` bracket structural drive.
//!
//! MISSION_PIPELINE_API_BOUNDARY G7 (Connor sign-off 2026-06-29): the unified,
//! framework-agnostic per-tick bracket. This test drives the FULL bracket —
//! `receive` (D0 recv-drain + entity-op application) and `send` (the D8 send-prep
//! sub-order + D9 core snapshot build + prepare + transmit) — synchronously on
//! the calling thread (the determinism/desync ORACLE shape: no worker threads),
//! over a real listening `LocalServerSocket`, across multiple ticks.
//!
//! ## What this proves
//!
//! - The bracket executes every line of the D0 + D8 + D9 sequence end-to-end
//!   against a `WorldMutType`/`WorldRefType` world (the demo world) without
//!   panicking — the preamble → scope → refresh strict sub-order, the core
//!   `SendStateView::build_needed_snapshot` assembler, and `prepare`/`transmit`.
//! - Handle integrity round-trips: `receive`/`send` take the three handles from
//!   their park-window slots and restore them, so subsequent unified ops
//!   (`current_tick`, `create_room`, …) keep working tick after tick.
//!
//! ## Deliberate scope (honest accounting)
//!
//! This drives the bracket with ZERO connected clients / ZERO replicated
//! entities. The ops that CREATE replicated entities *through* `PipelinedServer`
//! (`spawn_replicated` / `enable_replication`) arrive with G4/G5, and the bevy
//! adapter wiring that drives the bracket from `ReceivePackets`/`SendPackets`
//! against real clients is G8 — that suite is where end-to-end replication
//! THROUGH the bracket is exercised. The send-half's wire-byte fidelity is
//! already proven independently and byte-identically to resident in
//! `g9pre_resident_pipelined_byte_identity` + `g9pre_core_assembler_*` (the same
//! `build_needed_snapshot` → `prepare_send_job` → `transmit_send_job` primitives
//! the bracket composes). This test covers the *packaging + ordering + handle
//! lifecycle* the g9pre tests do not.

#![allow(unused_imports)]

use naia_demo_world::{Entity as DemoEntity, World as DemoWorld};
use naia_server::{
    transport::local::{LocalServerSocket, LocalTransportHub, Socket as ServerSocket},
    PipelinedServer, ServerConfig,
};

use naia_test_harness::protocol;

const FAKE_SERVER_ADDR: &str = "127.0.0.1:54399";

/// Build a listening `PipelinedServer<DemoEntity>` over a `LocalTransportHub`.
fn listening_server() -> PipelinedServer<DemoEntity> {
    let mut server: PipelinedServer<DemoEntity> =
        PipelinedServer::new(ServerConfig::default(), protocol());
    let hub = LocalTransportHub::new(FAKE_SERVER_ADDR.parse().unwrap());
    let server_socket = ServerSocket::new(LocalServerSocket::new(hub), None);
    server.listen(server_socket);
    server
}

#[test]
fn pipeline_bracket_drives_receive_then_send_over_many_ticks() {
    let mut server = listening_server();
    let mut world = DemoWorld::default();

    // A unified coord op between construction and the first bracket — proves the
    // op surface and the bracket share the same handle round-trip.
    let _room = server.create_room();

    let start_tick = server.current_tick();

    for _ in 0..8 {
        // D0: receive. With no clients connected, the recv-drain yields an empty
        // ReceiveOutput and the apply-to-world fast-path is taken.
        let output = server.receive(world.proxy_mut());
        assert!(
            output.is_empty(),
            "no clients connected → receive() must return an empty ReceiveOutput",
        );

        // D8 + D9: send. Runs preamble → scope → refresh, builds the (empty)
        // needed-set snapshot via the core assembler, prepares + transmits.
        server.send(&world.proxy());

        // Handle integrity: `current_tick` borrows the coord handle, which
        // `expect()`s the handle is present — so this panics if `receive`/`send`
        // failed to restore the handles to their slots.
        let _tick = server.current_tick();
    }

    // The bracket never drove a tick advance itself (that's the consumer/clock's
    // job), so the tick is unchanged — but the server is still fully operable.
    assert_eq!(
        server.current_tick(),
        start_tick,
        "the bracket must not advance the server tick on its own",
    );
    // Op surface still works after 8 bracket iterations (handles intact).
    let _room2 = server.create_room();
}
