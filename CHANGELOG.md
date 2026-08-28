# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/).

---

## [Unreleased]

### Changed

- **The entity waitlist is bounded, and can no longer panic the process.** A
  message carrying an `EntityProperty` whose entity is not yet in scope is parked
  on the waitlist. The existing `PER_ENTITY_WAITLIST_CAP` bounded only how many
  items may wait on *one* entity, and `RemoteEntity` ids are wire-supplied `u32`s
  -- so naming a fresh entity each time kept every per-entity queue at length one
  while the waitlist as a whole grew unbounded. It did not merely grow: waitlist
  handles came from a `KeyGenerator` that quarantines a freed key for 60s and
  *panics* rather than wrap when its width is exhausted, so 65536 such messages in
  a minute aborted the process outright. Two changes: a `TOTAL_WAITLIST_CAP` of
  4096 items with oldest-first eviction, and `WaitlistHandle` widened from `u16`
  to `u32`. Both are needed -- the handle namespace is a limit on handles issued
  per quarantine window, not on handles live at once, so capping the waiting set
  does not by itself prevent exhaustion. The handle is purely local and never
  serialised, so its width was a free choice. Eviction also no longer rescans the
  waiting set to find the entry it just popped, which had made each eviction cost
  O(cap) at exactly the point a peer could drive one per message.

- **Unanswered requests are bounded.** Every request received creates a routing
  entry in `GlobalResponseManager`, removed only when the application sends a
  response -- and nothing obliges an application to answer one. A peer could
  therefore make that map grow for the entire life of a connection. Both the
  server and client managers now cap outstanding response ids at 4096 and evict
  oldest-first; on the server the cap is per user, so one hostile connection
  cannot evict a well-behaved user's pending work. An evicted request degrades
  into the `Undeliverable` outcome `send_response` already models, rather than
  mis-routing a reply. Answered requests do not count toward the cap.

  Note the per-tick message cap was deliberately *not* extended to
  `incoming_requests`/`incoming_responses`. Both are drained every tick by the
  server and client receive paths, so neither accumulates across ticks, and that
  cap works by discarding already-acked messages -- adding a second lossy path
  would not have addressed the store that actually grew.

- **Reliable receivers now bound how far ahead of the stream a peer can reach.**
  `ReliableReceiver::buffer_message` instantiates one buffer slot per index
  between its oldest outstanding message and the `message_index` it is handed,
  and that index comes off the wire. A single packet claiming an index 32768
  ahead therefore grew the receive record to 32768 slots -- roughly 640 KB per
  channel per connection, from one packet. Receivers now derive a receive window
  from the channel's own `max_queue_depth`: a conforming peer's sender refuses to
  hold more than that many messages in flight and cannot retire an index until it
  is acked, so that depth is exactly the span it can legitimately span, and the
  tightest window that never rejects honest traffic. Channels are declared once in
  the shared `Protocol`, so both ends agree by construction; `max_queue_depth:
  None` opts out on both sides. Out-of-window messages are dropped with a warning
  rather than failing the connection, because the packet's ack is recorded before
  its payload is parsed.

- **`ChannelSettings::max_messages_per_tick` is documented as lossy.** It was
  already implemented as a truncation, and because a packet is acked before its
  payload is parsed, messages discarded by it are never retransmitted. On a
  reliable channel that is silent data loss, so the setting now carries an
  explicit warning; it bounds delivery throughput, not memory. The new receive
  window is what bounds memory. Internally the two travel together as
  `ReceiverCaps`, and the `with_cap` constructors became `with_caps`.

  Regression coverage for naia-lib/naia#165 was added alongside: `max_queue_depth`
  is what keeps a channel's index gaps encodable. `collect_messages` emits only
  messages that are due, so a large block of not-yet-due messages leaves a gap
  between consecutive written indices; past 32768 that gap reads as negative and
  `IndexedMessageWriter::write_message_index` panics encoding it as unsigned. The
  default depth of 1024 puts that out of reach.

- **Length-prefixed decoders no longer size their buffers from the wire.** `Vec`,
  `VecDeque`, `String`, and `Box<[u8]>` each read a length prefix and passed it
  straight to `with_capacity`, before decoding a single element. A six-byte packet
  claiming four billion elements made the process request 17 GB and abort -- an
  abort, so not even catchable as the `SerdeErr` the packet-drop path expects.
  The decode loop was always self-limiting (element decode fails once the reader
  runs dry), so only the pre-allocation was wrong: it is now bounded by what the
  reader could still contain, which needs no arbitrary cap. `BitReader` gained a
  `bits_remaining()` accessor for this. The `HashSet`/`HashMap` impls never
  pre-allocated and were unaffected.

- **Over-long variable-length integers are now rejected.** The continuation bit of
  a `SerdeInteger` variable encoding is wire-controlled, so a peer could keep the
  decode loop running past the width of the `u128` accumulator: 52 bytes of input
  panicked with `attempt to shift left with overflow` in debug, and silently
  corrupted the decoded value in release. A chunk starting at or beyond bit 128
  cannot belong to a value that ever fit, and is now a `SerdeErr`. A chunk that
  straddles bit 128 is still accepted -- the shift drops its high bits, matching
  the truncation the encoder already applied -- so maximal values round-trip
  unchanged.

- **Fragment reassembly validates every field it reads off the wire.** A
  `FragmentedMessage`'s id, index, total, and payload all come from the peer, and
  `FragmentReceiver` trusted them: an index past the declared total indexed past
  the end of the reassembly buffer, a declared total of zero left a buffer any
  index overran, a second fragment claiming index 0 hit an `unwrap` on a slot
  already filled, and repeating one non-zero index drove the received-count to the
  total so a sequence "completed" without fragment 0 ever arriving. Each is now a
  warn-and-drop. Payloads are also held keyed by index rather than in a `Vec`
  pre-sized to the declared total, so an id opened with a large total costs only
  what actually arrives.

- **The session listener no longer panics on malformed pre-auth requests.**
  `naia-server-socket`'s WebRTC session endpoint parses the incoming HTTP request
  before any authentication runs, and it did so with `.expect()`: a read error, or a
  header line that was not valid UTF-8, panicked the task. That task is spawned per
  connection on the shared executor, so a single unauthenticated packet from any
  peer that can reach the session port could take the server process down. Request
  parsing is now a separate `read_session_request` that returns `None` for anything
  malformed, and the connection gets a 404.

  The same path also buffered without bound. A peer could stream an unterminated
  header line, an endless run of well-formed headers, or declare an arbitrary
  `Content-Length`, and the server would allocate to match. Request lines are now
  capped at 8 KiB, total headers at 16 KiB, and the declared body at 64 KiB -- all
  far above any legitimate session request. Reported by @pbalcer (#168).

- **Unregistered wire net-IDs are now an error, not a panic.** `MessageKind::de`,
  `ChannelKind::de`, and `ComponentKind::de` read a net-ID off the wire and looked it
  up with `.expect()`. The tag is fixed-width, so any protocol whose registered count
  is not an exact power of two has encodable net-IDs with nothing behind them, and a
  remote peer can send one. The three registries' `net_id_to_kind` now returns
  `Result<_, SerdeErr>`; the decode fails, and the existing handling drops the packet
  and logs, as it already does for every other malformed-packet case.

  The reverse lookups (`kind_to_net_id`, `kind_to_builder`) keep panicking on purpose:
  their keys come from local application code, where a miss really is a missing
  `add_message()`/`add_channel()`/`add_component()` registration. Reported by
  @davehorner (#213).

- **URL rejection panics now say which URL was rejected.** `parse_server_url` rejected
  a path, query, or fragment with `panic!("")` after a `log::error!`, so a caller
  running without a logger got an empty panic message and no way to tell which of
  their config strings was wrong. These now carry the offending URL, matching the
  existing style in `naia-server`'s UDP transport, whose own `Url::parse` failure
  message gained the URL too. Two other empty panics -- in socket-address resolution
  and in the wasm data-channel setup -- were given messages as well. Reported by
  @bakcxoj (#185).

- **`CommandHistory` is now bounded.** Its only pruning happened in `replays`,
  which runs on server acknowledgement, so a client that kept predicting while
  acknowledgements stalled -- a hitching server, a stalled receive path, a long
  burst of loss -- grew the buffer without limit. `insert` now also evicts
  anything more than `max_ticks` behind the newest command. The bound is a span
  of ticks rather than a count of entries, so sparsely inserted commands still
  reach the full window back.

  `CommandHistory::default()` retains `DEFAULT_MAX_TICKS` (1200 ticks -- one
  minute at the default 50ms `tick_interval`), and `CommandHistory::new(max_ticks)`
  sets it explicitly. Existing `default()` callers need no change; the ceiling is
  far beyond any survivable round trip, so it only engages where the old
  behaviour would have leaked.

### Breaking changes

#### Transport sockets

- **`naia-client-socket` and `naia-server-socket` no longer expose trait objects.**
  `PacketSender`, `PacketReceiver`, `IdentityReceiver`, `AuthSender`, and `AuthReceiver`
  were traits whose only implementors lived in the same crate, handed out as
  `Box<dyn ...>`. They are now concrete types: the boxes, the `*Impl` suffixes, and the
  `Clone`-for-`Box` helper machinery are gone. `PacketReceiver` is an enum covering the
  plain and link-conditioned cases. Code holding `Box<dyn PacketSender>` (etc.) should
  hold `PacketSender` directly.
  These traits are not the ones you implement to add a transport — `naia_client::transport`
  and `naia_server::transport` still define those, and they are unchanged.

#### Identity tokens

- **`IdentityToken` is an opaque byte newtype, not a `String` alias.** It was
  `type IdentityToken = String`, so any string was a valid token. It is now
  `struct IdentityToken(Box<[u8]>)` with `generate()`, `from_bytes`, `as_bytes`, `len`,
  and `is_empty`. The free function `generate_identity_token()` is replaced by
  `IdentityToken::generate()`. Tokens no longer implement `Display`: to put one in the
  signaling payload, use `to_signaling_string()` (base64, URL-safe, no padding) and
  recover it with `from_signaling_string()`.

#### Authority

- **A refused `request_authority` now answers the requester.** When a client asked for
  authority on an entity another client already held, the server recorded the rejection
  locally but sent nothing back, leaving the refused client in `Requested` forever with
  no `EntityAuthDeniedEvent`. The server now sends `SetAuthority(Denied)` to the
  requester, which the client already knew how to turn into a denial event.

#### Handshake

- **The simple and advanced handshakers are merged.** `shared/src/handshake/{simple,advanced}/`,
  `client/src/handshake/{simple,advanced}_handshaker.rs`, and the server pair are replaced by
  one `HandshakeHeader` and one handshaker on each side. The two flows were always the same
  session negotiation; the advanced one only prepended an HMAC challenge/validate round-trip
  that validates the client's source address. That stage is now cfg-gated within the single
  handshaker. Behavior is unchanged for both builds.
- **Feature `advanced_handshake` renamed to `address_validation`** in `naia-shared`,
  `naia-client`, `naia-server`, and `naia-bevy-shared`. The name now describes what the
  feature does rather than how it was implemented. `transport_udp` still enables it.
- Client and server binaries must be built with matching `transport_udp` settings to
  interoperate, because `HandshakeHeader` is serialized by positional variant index. This
  was already true; it is now documented in the UDP book page.

#### Entities

- **`spawn_static_entity` removed.** Use `server.spawn_entity(world).as_static()` instead.
  Chain `.as_static()` before the first `.insert_component()` call; calling it after will panic.

#### Resources

- **`insert_static_resource` removed.** The `is_static: bool` parameter is now the third
  argument to `insert_resource`:
  - Dynamic: `server.insert_resource(world, value, false)`
  - Static: `server.insert_resource(world, value, true)`

- **`insert_resource` signature changed.** Previous signature was
  `insert_resource(world, value)` (always dynamic). New signature is
  `insert_resource(world, value, is_static: bool)`.

#### Events

- **`WorldEvents<E>` (client) renamed to `Events<E>`.** Update any type annotations or
  `use` imports that referenced `naia_client::WorldEvents`.

#### Rooms

- **`make_room` renamed to `create_room`.** All call sites must be updated.

#### Count methods

- **`resource_count()` renamed to `resources_count()`.** Noun is now plural, consistent
  with `users_count()` and `rooms_count()`.

- **`room_count()` on `UserRef` renamed to `rooms_count()`.** Same pluralisation rule.

#### Client replication config

- **`ReplicationConfig` enum removed from `naia_client`.** Client code that previously
  imported `naia_client::ReplicationConfig` must now import `naia_client::Publicity`
  (re-exported from `naia_shared`). The variants are unchanged: `Private`, `Public`,
  `Delegated`.

#### Messaging

- **`server.send_message` now returns `Result<(), NaiaServerError>`.** Callers that
  previously ignored the return value silently now receive an error if the user is not
  found.

#### EntityMut

- **`EntityMut::insert_components` (batch variant) removed from the server.** Use
  `insert_component` in a loop instead.

### Added

- **`entity_is_delegated` predicate on `Server<E>`.** Convenience equivalent to
  `server.entity_replication_config(e).map_or(false, |c| c.publicity.is_delegated())`.

- **`EntityMut::as_static()` builder method.** Replaces `spawn_static_entity`. Must be
  called before `insert_component` on entities that should be treated as static.

- **`server.give_authority(user_key, entity)` and `entity_mut.give_authority(user_key)`.**
  Server-initiated authority grant. Overrides any current holder (including the same user,
  making it idempotent). Requires the entity to be `Delegated` and in-scope for the target
  user; otherwise a silent no-op. Paired with `take_authority` to reclaim authority.
  Bevy adapter: `entity_commands.give_authority(&mut server, &user_key)`.

- **`server.take_authority(entity)` and `entity_mut.take_authority()`.**
  Reclaims server authority from whatever client currently holds it. Sends
  `SetAuthority(Denied)` to the previous holder and `SetAuthority(Available)` to
  any observers. Bevy adapter: `entity_commands.take_authority(&mut server)`.

- **Reconnect edge-case handling.** Clients that disconnect and reconnect mid-session
  now correctly re-receive all in-scope entities and replicated resources on reconnect.
  Previously, a rapid disconnect/reconnect could leave the client with a stale
  entity set.

### Changed

- **Crate names kebab-cased.** The three internal test/tool crates were renamed for
  consistency with Rust conventions:
  - `naia_npa` → `naia-npa`
  - `naia_bevy_npa` → `naia-bevy-npa`
  - `naia_spec_tool` → `naia-spec-tool`
  Binary file names (snake_case) are unchanged.

- **`transport::local` hub debug output silenced.** Three `println!` calls in
  `LocalTransportHub` were replaced with `log::debug!`. Local-transport noise no
  longer appears in server stdout during tests or production use.

### Fixed (V2 audit, 2026-05-09)

- **CRITICAL — UB transmute in local transport receivers.** `LocalServerReceiver`
  and `LocalClientReceiver` extended the lifetime of a `MutexGuard`-owned buffer
  via `std::mem::transmute`. Both structs now own their last-received payload as
  `Option<Box<[u8]>>`, eliminating the transmute entirely.

- **Handshake address-to-timestamp map unbounded.** Changed from `HashMap` to
  `CacheMap<_, _, MAX_PENDING_CONNECTIONS=1024>` to prevent OOM from spoofed
  source-address floods before authentication completes.

- **Handshake `delete_user` scan-by-value gap.** When a user disconnected before
  completing the identify step, their `been_handshaked_users` entry was left
  orphaned. Fixed with a `retain()` scan on `None` address.

- **`on_delivered_migrate_response` dead stub removed.** The function had two
  incorrect magic values in its TODO body; it was not called anywhere. Removed to
  avoid a future confusion hazard.

- **`user()` panics on stale key.** Added `user_opt` and `user_mut_opt` on
  `Server<E>`, `WorldServer<E>`, and `MainServer` so callers can avoid the panic
  when a `UserKey` may be stale.

- **Pending-auth timeout.** Connections that completed the network handshake but
  whose application never called `accept_connection` / `reject_connection` within
  `ServerConfig::pending_auth_timeout` (default 10 s) are now auto-rejected with
  a warning log.

- **`host_engine` receive on unknown entity panicked.** Changed to `warn!` + discard;
  reordered packets from a lagging client after entity despawn no longer crash the server.

- **`url_str_to_addr` panics lacked context.** All five `panic!("")` calls now
  include the offending URL string.

- **Safety comments on all `unsafe` blocks.** 20 unsafe sites across server, client,
  shared, socket, and adapter crates now carry `// Safety:` comments explaining
  the invariant that justifies each unsafe use.
