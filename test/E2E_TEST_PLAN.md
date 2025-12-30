Below is a reorganized, compressed version of your BDD suite plus a suggested file structure.

I’ve grouped related headings into larger “domains” and kept each scenario one-to-one with your originals, just with tighter wording so a Cursor agent can implement them directly.

---

## 1. Connection, Auth & Identity

### 1.1 Connection & User Lifecycle

* **Basic connect/disconnect lifecycle**
  Given an empty server; when A connects, then B connects, then A disconnects; then connect events are [A, B], only B remains connected, and all entities/scope for A are cleaned up.

* **Connect event ordering is stable**
  Given a server; when A connects then B connects; then exactly two connect events appear in order [A, B] with no duplicates.

* **Disconnect is idempotent and clean**
  Given A and B connected; when A disconnects and later a duplicate/connection-lost for A is processed; then only one disconnect event for A is exposed, A is fully removed from users and scoping, and B never sees ghost entities from A.

### 1.2 Auth

* **Successful auth with `require_auth = true`**
  Given `require_auth = true` and an auth handler accepting certain credentials; when A connects with valid auth; then server emits one auth event then one connect event for A, A becomes connected, and scoped entities replicate.

* **Invalid credentials are rejected**
  Given `require_auth = true` and an auth handler rejecting bad credentials; when A connects with invalid auth; then server emits an auth event but no connect event, A never appears as connected, and receives no replication.

* **Auth disabled connects without auth event**
  Given `require_auth = false`; when A connects (with or without auth payload); then no auth event is emitted, a connect event is emitted, and A becomes a normal connected user.

* **No replication before auth decision**
  Given `require_auth = true` and existing in-scope entities; when A connects and auth is delayed; then until auth is accepted, A is not treated as connected and receives no replicated entities or data-plane events.

* **No mid-session re-auth or identity swap**
  Given A authenticated and connected; when A sends additional auth payload mid-session trying to change identity; then identity does not change, the attempt is ignored or rejected (optionally causing disconnect), and no silent identity swap occurs.

### 1.3 Connection Errors, Rejects & Timeouts

* **Server capacity-based reject produces RejectEvent, not ConnectEvent**
  Given server at max concurrent users; when another client tries to connect; then a reject indication is emitted, no connect event is emitted, and the client remains/ends disconnected.

* **Client disconnects due to heartbeat/timeout**
  Given configured heartbeat/timeout; when traffic stops longer than timeout; then both sides eventually emit a timeout disconnect event and all entities for that connection are cleaned up.

* **Protocol or handshake mismatch fails before connection**
  Given server expecting a specific handshake/protocol; when client connects with incompatible handshake or version; then handshake fails, an error/reject is surfaced, no connect event or gameplay state is created, and client sees a clear error.

### 1.4 Identity Token & Handshake Semantics

* **Malformed or tampered identity token is rejected cleanly**
  Given server expecting well-formed identity tokens; when client uses a malformed/tampered token; then handshake fails, client never becomes connected, an error/reject is surfaced, and no half-connected state remains.

* **Expired or reused identity token obeys documented semantics**
  Given a token valid only once or within a time window; when client uses an expired or already-used token; then server enforces the documented rule (e.g., explicit rejection or forced new identity) and does not silently accept it as a fresh session.

* **Valid identity token round-trips from server generation to client use**
  Given server generates a token via public API and passes it to a client; when that client uses it to connect; then handshake succeeds, connection is associated with that identity as documented, and no extra hidden state is needed.

---

## 2. Rooms, Scope, Snapshot & Join

### 2.1 Rooms & Scoping

* **Entities only replicate when room & scope match**
  Given Room1 with A and Room2 with B; when server spawns public E in Room1 and public F in Room2; then A sees only E, B sees only F, and server room state is E∈Room1, F∈Room2.

* **Moving a user between rooms updates scope**
  Given E public in Room1, A in Room1, B in Room2; when server moves B from Room2 to Room1; then B spawns E, A continues to see E, and B never sees entities that exist only in Room2.

* **Moving an entity between rooms updates scope**
  Given A and B in Room1 and public E in Room1 visible to both; when server moves E to Room2; then A and B despawn E, and clients in Room2 see E.

* **Custom viewport scoping function (position-based scope)**
  Given A and B in same room, entity E with Position, and per-client viewports; when E’s Position moves from A’s viewport region into B’s; then A initially sees E then despawns it on exit, B initially does not see E then spawns it on entry.

### 2.2 Multi-Room & Advanced Scoping

* **Entity belonging to multiple rooms projects correctly to different users**
  Given E in both RoomA and RoomB; when U1 is only in RoomA, U2 only in RoomB, U3 in both; then U1 sees E once, U2 sees E once, U3 sees E once, and removing E from one room only affects users whose visibility depended on that room.

* **Manual user-scope include overrides room absence**
  Given E in RoomA and U not in RoomA; when server manually includes E in U’s user scope; then U sees E while override is active, and despawns E when override is removed (even though E stays in RoomA).

* **Manual user-scope exclude hides an entity despite shared room**
  Given E and U both in RoomA; when server explicitly excludes E from U’s scope; then U does not see E while override is active, and E reappears for U once override is removed.

* **Publish/unpublish vs spawn/despawn semantics are distinct**
  Given E exists on server; when server publishes E to a room, later unpublishes it, then finally despawns it; then clients see E appear on publish, disappear on unpublish, and never see E again after despawn even if re-published as a new lifetime.

### 2.3 Join-In-Progress & Reconnect

* **Snapshot on join-in-progress**
  Given Room with entities E1–E3 already replicated to existing clients; when B connects and joins Room; then B’s initial snapshot includes all in-scope entities with current component values (no history replay), and existing clients’ views are untouched.

* **Clean reconnect**
  Given A and B connected and seeing same entities; when A disconnects (graceful or simulated loss) and later reconnects as same or new logical player per chosen model; then after rejoin A’s world matches server’s current state (and B’s) with no ghost or missing entities.

### 2.4 Initial Snapshot & Late-Join Behaviour

* **Late-joining client receives full, current snapshot of all in-scope entities**
  Given E1–E3 exist, updated, and published in RoomR with A observing; when B joins RoomR; then B’s first world view contains E1–E3 with all components at current values, with no partially-populated entities.

* **Late-joining client does not see removed components or despawned entities from history**
  Given entities were spawned, modified, some components removed, some entities despawned before B connects; when B joins; then B only sees currently alive entities with current components, and no historical ghost entities/components.

* **Entering scope mid-lifetime yields consistent snapshot without historical diffs**
  Given E existed and changed while A was out of scope; when A’s scope changes so that E becomes in-scope; then A first sees E as a coherent snapshot of its current state, without replaying older intermediate diffs.

* **Leaving scope vs despawn are distinguishable and behave consistently**
  Given A sees E; when E leaves A’s scope but is not despawned; then A sees E disappear without a “despawn” lifetime event, and later re-entering scope shows E again with fresh snapshot; when E is truly despawned, all scoped clients see a despawn and E never reappears.

* **Reconnect always yields a clean snapshot independent of prior connection state**
  Given A connects, sees entities, then disconnects; when A reconnects and rejoins rooms; then A receives a fresh snapshot based solely on current server state with no accidental reuse of old client-side mappings.

---

## 3. Entities, Components, Lifetime & Logical Identity

### 3.1 Entity & Component Replication

* **Server-spawned public entity replicates to all scoped clients**
  Given A and B in same room; when server spawns public E with Position; then A and B both see E with same Position.

* **Private replication: only owner sees it**
  Given A and B in same room; when A spawns E with owner-only/private replication; then A (and server) see E, but B never sees E or its components.

* **Component insertion after initial spawn**
  Given E with Position replicated to A and B; when server inserts new component Velocity; then A and B see E with Velocity added and Position unchanged, and any later-joining client sees E with both components.

* **Component updates propagate consistently across clients**
  Given E with Position and Health visible to A and B; when server updates both components across ticks; then A and B never observe impossible combinations and converge to same final (Position, Health) as server.

* **Component removal**
  Given E with Position and Health visible to A and B; when server removes Health; then A and B see E without Health (Position intact), and joiners see E without Health.

* **Despawn semantics**
  Given E visible to A and B; when server despawns E; then A and B despawn E, no further updates for E are processed client-side, and late packets referencing E are ignored safely.

* **No updates before spawn and none after despawn**
  Given entities spawned, updated, and despawned under packet reordering; then each client only sees updates after a spawn for that entity and never sees updates/messages referencing the entity after its despawn.

### 3.2 Logical Identity & Multi-Client Consistency

* **Stable logical identity across clients in steady state**
  Given A spawns public E replicated to B; when A mutates E’s components over time; then whenever both see E, they refer to the same logical entity and observe the same component values.

* **Late-joining client gets consistent identity mapping**
  Given A already seeing E in a room; when B later joins that room; then B’s initial snapshot includes E, and subsequent mutations to E are consistently observed on both A and B as the same logical entity.

* **Scope leave and re-enter semantics (decided model)**
  Given E public and A initially in scope; when A leaves E’s scope and despawns E, then later re-enters scope; then behavior matches the chosen model (new lifetime vs reappearance of same logical entity), and the test asserts the chosen contract.

### 3.3 Event Ordering & Cleanup

* **Long-running connect/disconnect and spawn/despawn cycles do not leak**
  Given a test that repeatedly connects/disconnects clients and spawns/despawns entities over many cycles; when it completes; then server and clients report zero users/entities, and internal counts remain bounded (no leaks).

---

## 4. Ownership & Delegation

### 4.1 Delegation & Authority

* **Client-owned spawn grants authority to that client**
  Given server supports delegated entities; when A spawns E as client-owned; then server records A as owner, emits authority-grant events, and accepts component updates from A for E as authoritative.

* **Owner updates propagate; non-owners cannot control delegated entity**
  Given A owns delegated E and B sees E; when A updates E; then A and B see updated state; when B attempts to update E directly; then those updates are ignored and authoritative state remains driven by A/server.

* **Delegation request for non-delegatable entity is denied**
  Given server-owned non-delegatable E; when A requests delegation/authority over E; then ownership does not change, no grant event is emitted, and A’s direct control attempts are ignored.

* **Server can revoke authority (reset)**
  Given A owns delegated E; when server revokes E’s authority; then an authority-reset event is emitted, E becomes server-owned (or safe default), and further updates from A for E are ignored while replication continues normally.

* **Delegated owner disconnect cleanup**
  Given A owns delegated E and B observes E; when A disconnects; then server resets E’s authority to a safe state, keeps E alive and replicated to appropriate clients, and future delegation can proceed without stale ties to A.

### 4.2 Advanced Ownership / Delegation

* **Mixed ownership per component respects authority boundaries**
  Given E with some server-owned components and some delegated to A; when A mutates both; then only delegated components change, and server-owned components ignore A’s modifications.

* **Ownership transfer from one client to another**
  Given E initially owned by A; when server transfers ownership to B; then A loses ability to update E, B gains it, B’s updates are applied, and A’s subsequent updates are ignored.

* **Concurrent conflicting updates respect current owner**
  Given E with ownership that can change; when A and B both send updates and server switches ownership from A to B during the period; then updates from A before transfer are applied, updates from A after transfer are ignored, and B’s post-transfer updates are applied.

* **Authority revocation races with pending updates**
  Given A owns E and has in-flight updates; when server revokes A’s authority; then updates arriving after revocation are discarded, and final replicated state reflects only pre-revocation updates.

### 4.3 Delegation & Scoping Edge Cases

* **Delegation to an out-of-scope client behaves predictably**
  Given E not in A’s scope; when server delegates authority to A or accepts delegation from A; then behavior matches the chosen contract (e.g., either E is first brought into scope or A’s updates are rejected until in-scope), and test asserts that contract.

* **Component-level grant and later reset for delegated authority**
  Given E with component `C_owned` delegated to A and others server-owned; when server issues authority reset for `C_owned`; then A’s later changes to `C_owned` are ignored and server resumes sole authority, with visibility matching pre- vs post-reset states.

* **Owner removed from scope retains or loses authority consistently**
  Given A owns delegated E and B observes E; when E is removed from A’s scope but remains alive; then system either automatically revokes authority from A or lets A retain authority while out-of-scope, and test locks the chosen behavior (including handling of updates from A).

---

## 5. Messaging, Channels & Request/Response

### 5.1 Reliable Messaging & Channels

* **Reliable server-to-clients broadcast respects rooms**
  Given RoomR with A,B and RoomS with C; when server broadcasts a reliable message on a channel to RoomR; then A and B each receive exactly one copy in-order on that channel, and C receives none.

* **Reliable point-to-point request/response**
  Given A connected and server listening for request type; when A sends a reliable request and server replies reliably only to A; then A sees exactly one response after its request, no other client sees it, and from A’s perspective response comes after its request.

* **Per-channel ordering**
  Given Channels 1 and 2 and shared scope between A and B; when server sends M1,M2,M3 on Channel1 and N1,N2 on Channel2 in that order; then on A and B each channel preserves its own order (M1→M2→M3; N1→N2) regardless of interleaving between channels.

### 5.2 Channel Semantics

* **Ordered reliable channel keeps order under latency and reordering**
  Given ordered reliable channel; when server sends A,B,C and transport reorders packets; then client receives exactly one A,B,C in order A→B→C.

* **Ordered reliable channel ignores duplicated packets**
  Given ordered reliable channel; when transport duplicates packets for A,B; then client still surfaces exactly one A and one B in order with no duplicates.

* **Unordered reliable channel delivers all messages but in arbitrary order**
  Given unordered reliable channel; when server sends A,B,C under latency/reordering; then client receives exactly one A,B,C in some order not guaranteed to match send order.

* **Unordered unreliable channel shows best-effort semantics**
  Given unordered unreliable channel with configurable loss; when server sends a sequence at fixed rate; then with no loss all messages arrive once; with configured loss some messages never arrive and are not retried.

* **Sequenced reliable channel only exposes the latest message in a stream**
  Given sequenced reliable “current state” stream; when server sends S1,S2,S3 for same stream under delay/reordering; then client may drop older states but ends up exposing S3 only and never reverts to S1 or S2 after seeing S3.

* **Sequenced unreliable channel discards late outdated updates**
  Given sequenced unreliable channel; when server sends U1..U10 and network delivers U3,U4 after U8,U9; then client drops U3,U4 and only applies newest sequence, never reverting.

* **Tick-buffered channel groups messages by tick**
  Given tick-buffered channel with known tick rate; when server sends messages tagged with ticks T,T+1,T+2 with packet reordering; then client exposes buffered messages grouped by tick and never surfaces messages for T+1 before it has processed tick T.

* **Tick-buffered channel discards messages for ticks that are too old**
  Given tick-buffered channel with sliding window; when messages for ticks T,T+1,T+2 are sent but tick T arrives long after client has advanced beyond T; then late tick-T messages are discarded and not applied to current state.

### 5.3 Request / Response Semantics

* **Client-to-server request yields exactly one response**
  Given typed request/response; when client sends request R with ID and server processes it; then client eventually observes exactly one matching response for that ID, even under packet duplication.

* **Server-to-client request yields exactly one response**
  Given server sending requests to client; when server sends request Q and client replies; then server observes exactly one matching response for Q with no duplicates even if packets duplicate.

* **Request timeouts are surfaced and cleaned up**
  Given client sends request R; when server never replies and timeout elapses; then client surfaces a timeout result for R, releases tracking, and does not leak resources.

* **Requests fail cleanly on disconnect mid-flight**
  Given in-flight request R from client; when connection drops before response; then both sides eventually mark R failed/cancelled, do not leak state, and ignore any late response for R after reconnect.

### 5.4 Request/Response Concurrency & Isolation

* **Many concurrent requests from a single client remain distinct**
  Given one client issuing many concurrent requests; when server processes them in arbitrary order and replies out-of-order; then client gets exactly one response per request and correctly matches responses to original requests without collisions.

* **Concurrent requests from multiple clients stay isolated per client**
  Given multiple clients issuing overlapping request IDs (e.g., each uses 0,1,2); when server handles all and responds; then each client only sees responses to its own requests and no response is misrouted to another client.

* **Response completion order is well-defined and documented**
  Given multiple requests from one client completed in a different order than they were sent; when client observes responses; then they arrive in the order promised by the contract (e.g., completion order), and the test forces a send-order/completion-order mismatch to verify behavior.

---

## 6. Time, Ticks, Transport, Limits & Observability

### 6.1 Time, Transport & Determinism

* **Deterministic replay of a scenario**
  Given fully scripted scenario and deterministic clock/seed; when scenario executes twice; then externally observable events and world states on all clients are identical across runs.

* **Robustness under simulated packet loss**
  Given A and B seeing replicated E; when server updates E while test transport drops a substantial fraction of packets; then after loss subsides both clients converge to server’s latest E state without permanent divergence.

* **Out-of-order packet handling does not regress to older state**
  Given E updated monotonically; when some packets carrying older states are delayed until after newer states; then clients never regress to older state once newer state applied, and eventually report latest state.

### 6.2 Tick / Time / Command History

* **Server and client tick indices advance monotonically**
  Given server and client with matching tick rates; when simulation runs; then both server tick and client’s notion of server tick advance monotonically, never decreasing or rolling back.

* **Pausing and resuming time does not create extra ticks**
  Given deterministic time source; when time is paused (no tick advancement) then resumed; then no ticks are generated during pause and progression resumes smoothly from last tick index.

* **Command history preserves and replays commands after correction**
  Given client sends per-tick input and server sends authoritative state; when client receives corrected state for earlier tick while holding newer commands; then client replays newer commands in order on corrected state and reaches same final state as if correction had been there from start.

* **Command history discards old commands beyond its window**
  Given bounded command history; when many ticks pass and commands are inserted; then commands older than window are discarded, and late corrections for ticks outside window do not attempt to replay discarded commands.

### 6.3 Wraparound & Long-running Behaviour

* **Tick index wraparound does not break progression or ordering**
  Given deterministic time and known tick counter max; when server and client tick through wraparound; then tick ordering stays correct, channels/tick-buffer semantics still hold, and no panics/invalid state occur.

* **Sequence number wraparound for channels preserves ordering semantics**
  Given ordered channel with wrapping sequence numbers; when enough messages force wrap; then ordered semantics still hold across wrap and later messages are still treated as newer.

* **Long-running scenario maintains stable memory and state**
  Given long scenario with frequent connects/disconnects, spawns/updates/despawns, and messages; when test finishes; then user/entity counts and buffer sizes remain bounded, and no ghost users/entities remain.

### 6.4 Link Conditioner Stress

* **Extreme jitter and reordering preserve channel contracts**
  Given link conditioner with high jitter and reordering; when sending messages and replication updates over ordered/unordered/sequenced/tick-buffered channels; then each channel still satisfies its documented ordering/reliability/latest-only semantics.

* **Packet duplication does not surface duplicate events**
  Given link conditioner that duplicates packets at high rate; when server sends entity updates and messages; then clients never observe duplicate spawn/despawn/message/response events, and state does not regress even if older duplicates arrive after newer packets.

### 6.5 MTU, Fragmentation & Compression

* **Large entity update that exceeds MTU is correctly reassembled**
  Given E whose update exceeds single MTU; when server sends full update; then client applies a complete coherent update only after all fragments arrive, never partial component state, even with delayed/duplicated fragments.

* **Fragment loss causes older state until a full later update**
  Given repeated large updates for E with fragmentation; when one update loses a fragment but a later full update arrives intact; then client stays at previous valid state until later full update is applied, never applying a partially missing update.

* **Compression on/off does not change observable semantics**
  Given scenario with entities/messages; when run once with compression off and once on; then sequence of API-visible events, entity states, and messages is identical between runs (only bandwidth differs).

### 6.6 Config, Limits & Edge Behaviour

* **Reliable retry/timeout settings produce defined failure behaviour**
  Given reliable channel with limited retries/timeouts; when server sends reliable message over link that can’t deliver within budget; then sender surfaces a clear failure/timeout, stops retrying, and system does not hang or leak.

* **Minimal retry reliable settings produce clear delivery failure semantics**
  Given reliable channel with extremely low retries/timeouts; when messages cannot be delivered within constraints; then sender reports “delivery failed” or timeout, stops retrying, and no internal state is left stuck.

* **Very aggressive heartbeat/timeout still leads to clean disconnect**
  Given very small heartbeat/timeout values; when traffic briefly pauses or link is stressed; then connection may time out but disconnect remains clean (events emitted, state cleared) with no partial user state.

* **Tiny tick-buffer window behaves correctly for old ticks**
  Given tick-buffer with very small window; when messages tagged with old ticks arrive after window advanced; then they are dropped according to semantics and never applied to current state or regress tick index.

* **Switching a channel from reliable to unreliable (or ordered to unordered) only changes documented semantics**
  Given two runs of same scenario, one with channel reliable/ordered, another unreliable/unordered; when comparing; then only the documented differences (loss/reordering) appear, with no unintended effects like instability or desync.

### 6.7 Observability: Ping & Bandwidth

* **Reported ping/RTT converges under steady latency**
  Given link with fixed RTT and low jitter/loss; when client/server exchange several heartbeats; then reported ping/RTT converges near configured latency and is never negative or wildly unstable.

* **Reported ping remains bounded under jitter and loss**
  Given link with significant jitter and modest loss; when running; then ping/RTT fluctuates but stays finite, non-negative, and below a reasonable ceiling (no overflow/garbage values).

* **Bandwidth monitor reflects changes in traffic volume**
  Given bandwidth metric; when system alternates between high traffic and near-idle; then reported bandwidth rises during high activity and drops during idle, without staying stuck at stale values.

* **Compression toggling affects bandwidth metrics but not logical events**
  Given scripted replication/messages; when run once with compression off and once on; then compressed run shows fewer bytes sent, while logical events and world states stay identical.

---

## 7. Protocol, Types, Serialization & Version Skew

* **Serialization failures are surfaced without poisoning the connection**
  Given a type that can be forced to fail (de)serialization; when such a failure occurs; then side detecting error surfaces an appropriate error, ignores the failing message/entity, and connection continues functioning for other traffic.

* **Multi-type mapping across messages, components, and channels**
  Given protocol with multiple message types on multiple channels and multiple component types; when server/client exchange mixed messages and entity updates; then each received message arrives as correct type on correct channel, each update as correct component type, and nothing is misrouted/decoded as wrong type.

* **Channel separation for different message types**
  Given messages bound to ChannelA vs ChannelB; when server sends A1,A2 on A and B1,B2 on B; then client observes A1,A2 only through ChannelA API and B1,B2 only through ChannelB API.

* **Protocol type-order mismatch fails fast at handshake**
  Given server/client with intentionally mismatched protocol definitions (type ID ordering differs); when client connects; then handshake fails early with clear mismatch outcome, no gameplay events are generated, and both sides clean up.

* **Client missing a type that the server uses**
  Given server protocol with an extra type not in client protocol; when client connects and server uses that type; then either connection is rejected as incompatible or server avoids sending unsupported type; in either case client never crashes or enters undefined state.

* **Safe extension: server knows extra type but still interoperates**
  Given server protocol defines extra message type `Extra` beyond baseline while client only knows baseline; when client connects; then behavior follows documented rule: either `Extra` is never sent to that client while baseline works, or connection is rejected as incompatible.

* **Schema incompatibility produces immediate, clear failure**
  Given server/client with incompatible schemas for a shared type; when they attempt to exchange that type; then incompatibility is detected and surfaced as error/disconnect before corrupted values reach public API.

---

## 8. Events, World Integration & Misuse Safety

### 8.1 Server Events API (naia_server::Events)

* **Inserts/updates/removes are one-shot and non-duplicated**
  Given server spawns E, updates a component, then removes it in one tick; when main loop calls `take_inserts`, `take_updates`, `take_removes` once; then each change appears exactly once and subsequent calls that tick return nothing for those changes.

* **Component update events reflect correct multiplicity per user**
  Given component replicated to multiple users; when server changes component once; then `take_updates` returns one event per in-scope user with no duplicates or missing entries.

* **Message events grouped correctly by channel and type**
  Given multiple message types from multiple users across multiple channels in one tick; when Events API drains messages; then grouping matches documented structure (by channel/type/user), each message appears once, and second call in same tick yields none.

* **Request/response events via Events API are drained and do not reappear**
  Given multiple client requests and server responses in a tick; when Events API drains request/response events; then each appears exactly once and does not reappear later that tick, with no silent loss.

### 8.2 Client Events API Semantics

* **Client spawn/insert/update/remove events occur once per change and drain cleanly**
  Given E is spawned, component inserted, updated, then removed while in A’s scope; when A processes events for those ticks; then A sees one spawn, one insert, appropriate updates, and one remove, and already-drained events do not reappear.

* **Client never sees update or remove events for entities that were never in scope**
  Given entities created/destroyed entirely while A is out of scope; when A drains events; then A sees no events for those entities.

* **Client never sees update or insert events before seeing a spawn event**
  Given E is spawned then updated/extended; when A processes events; then first event for E is spawn (plus possible initial inserts) and no update/remove is seen before spawn.

* **Client never sees events after despawn for a given entity**
  Given E is spawned, updated, then despawned while in A’s scope; when A processes events after despawn, including under packet reordering; then E generates no further events.

* **Client message events are grouped and typed correctly per channel**
  Given A receives multiple message types over multiple channels in one tick; when A drains message events; then each message appears once with correct type and bound to correct channel.

* **Client request/response events are drained once and matched correctly**
  Given multiple server-to-client requests and client responses across ticks; when client processes its request/response events; then each incoming request and outgoing response appears once, is matchable to correct logical ID/handle, and does not reappear.

### 8.3 World Integration via WorldMutType / WorldRefType

* **Server world integration receives every insert/update/remove exactly once**
  Given fake world wired via `WorldMutType`; when entities spawn, components change, and entities despawn; then fake world sees each operation exactly once, in same order as Naia’s internal world.

* **Client world integration stays in lockstep with Naia’s view**
  Given fake client world updated from client events; when server spawns/updates/despawns entities; then at each tick integrated world has same entities and component values as Naia client.

* **World integration cleans up completely on disconnect and reconnect**
  Given clients connect, cause world changes, then disconnect and later reconnect; when inspecting fake world after each cycle; then it only contains entities for currently connected sessions and in-scope rooms, with no leftover entities from past sessions.

### 8.4 Robustness Under API Misuse (Non-Panicking, Defined Errors)

* **Accessing non-existent entity yields safe failure, not panic**
  Given no entity with a certain ID; when code attempts to access it via read/write APIs; then APIs return “not found”/`None`/error without panicking or corrupting state.

* **Accessing an entity after despawn is safely rejected**
  Given E was spawned then despawned; when code attempts to read/mutate E after despawn; then calls fail gracefully and do not recreate E or panic.

* **Mutating out-of-scope entity for a given user is ignored or errors predictably**
  Given E not in A’s scope; when A tries to mutate E via client APIs or server applies per-user operation assuming A sees E; then Naia either ignores the operation or returns a defined error, without corrupting scoped state.

* **Sending messages or requests on a disconnected or rejected connection is safe**
  Given a connection that is disconnected or rejected; when code sends a message/request on it; then attempt is ignored or returns clear error, and does not resurrect connection or panic.

* **Misusing channel types (e.g., sending too-large message) yields defined failure**
  Given a channel with constraints (e.g., max message size); when caller sends a violating message; then Naia surfaces a defined error/refusal and does not fall into undefined behavior or corruption.

---

## 9. Integration & Transport Parity

* **Core replication scenario behaves identically over UDP and WebRTC**
  Given simple multi-client scenario (spawn/update/despawn and some messages); when run once over UDP and once over WebRTC with equivalent link conditions; then externally observable events (connects, spawns, updates, messages, despawns, disconnects) are identical modulo timing.

* **Transport-specific connection failure surfaces cleanly**
  Given WebRTC transport configured so ICE/signalling fails; when client attempts to connect; then connection eventually fails with clear error, no partial user/room state is created on server, and client doesn’t get stuck half-connected.

* **Integrated “everything at once” scenario stays consistent and error-free**
  Given scenario with multiple rooms, manual scope overrides, mixed ownership entities, multiple channel types, concurrent request/response, moderate jitter/loss/duplication/reordering, and long runtime; when scenario completes; then all per-feature contracts (no duplicates, correct per-channel ordering, correct scoping/ownership, correct request/response matching, no updates before spawn or after despawn) hold simultaneously, no panics/asserts occur, and final client/server world states match intended authoritative state.

---

## File / Module Structure

A reasonable way to map these domains into files (Rust-style) is:

```text
tests/
    connection_auth_identity.rs        # Domain 1
    rooms_scope_snapshot.rs           # Domain 2
    entities_lifetime_identity.rs     # Domain 3
    ownership_delegation.rs           # Domain 4
    messaging_channels.rs             # Domain 5.1–5.2
    request_response.rs               # Domain 5.3–5.4
    time_ticks_transport.rs           # Domains 6.1–6.4
    mtu_compression_limits.rs         # Domains 6.5–6.6
    observability_metrics.rs          # Domain 6.7
    protocol_schema_versioning.rs     # Domain 7
    server_events_api.rs              # Domain 8.1
    client_events_api.rs              # Domain 8.2
    world_integration.rs              # Domain 8.3
    robustness_api_misuse.rs          # Domain 8.4
    transport_parity.rs               # First two tests in Domain 9
    kitchen_sink_integration.rs       # Last test in Domain 9
```

The important part is that each file owns a coherent domain slice so a Cursor agent can work through them systematically without cross-cutting concerns.
