# Request / Response

naia supports typed request/response pairs over reliable bidirectional channels.
This is useful for one-shot operations where the caller needs a reply: fetching a
leaderboard entry, submitting an item purchase, or loading level data.

---

## Defining a request/response pair

A request type derives `Message` and implements the `Request` trait, which
associates it with its response type. The response type derives `Message` and
implements the `Response` marker trait:

```rust
use naia_shared::{Message, Request, Response};

/// The request struct — carries the query parameters.
#[derive(Message)]
pub struct FetchScore {
    pub player_id: u32,
}

impl Request for FetchScore {
    type Response = FetchScoreResponse;
}

/// The response struct — carries the answer.
#[derive(Message)]
pub struct FetchScoreResponse {
    pub score: u32,
    pub rank:  u32,
}

impl Response for FetchScoreResponse {}
```

Register the request type in the `Protocol` builder using `add_request`:

```rust
Protocol::builder()
    .add_request::<FetchScore>()
    .build()
```

> **Note:** You register the **request** type only — the response type is
> discovered automatically through the `Request::Response` associated type.

---

## Sending a request (client)

```rust
// Returns Ok(ResponseReceiveKey) on success, Err if the channel is full.
let response_key = client.send_request::<RequestChannel, _>(
    &FetchScore { player_id: 42 },
)?;

// Store `response_key` — you need it to match the reply when it arrives.
global.pending_requests.insert(response_key, PendingKind::FetchScore);
```

`send_request` returns a `ResponseReceiveKey` you use to match the reply.
Requests travel over a bidirectional reliable channel — register the channel
with `ChannelDirection::Bidirectional` and `ChannelMode::OrderedReliable`.

---

## Handling the request (server)

```rust
// In your request event system:
for (user_key, response_send_key, request) in
    events.read::<RequestChannel, FetchScore>()
{
    let score = db.lookup_score(request.player_id);
    server.send_response(
        &response_send_key,
        &FetchScoreResponse { score, rank: 1 },
    );
}
```

The server receives a `response_send_key` alongside the request. Pass it back
to `send_response` to route the reply to the correct client.

---

## Receiving the response (client)

```rust
// In your response event system:
for (response_receive_key, response) in
    events.read::<RequestChannel, FetchScore>()
{
    if let Some(kind) = global.pending_requests.remove(&response_receive_key) {
        println!("Score: {}, Rank: {}", response.score, response.rank);
    }
}
```

---

## Bidirectional requests

Either side can send requests. In the Bevy demo, both the server and client
issue requests to each other:

```rust
// Server sending a request to a client:
let response_key = server.send_request::<RequestChannel, _>(&user_key, &request)?;

// Client handling a request from the server and sending a response:
for (response_send_key, request) in events.read::<RequestChannel, BasicRequest>() {
    client.send_response(&response_send_key, &BasicResponse { /* … */ });
}
```

---

## TTL and disconnect cleanup

Pending requests are automatically cancelled when the connection drops. Unmatched
`ResponseReceiveKey` values become invalid and will not fire any event after
disconnect.

> **Tip:** Use request/response for infrequent operations (level transitions,
> purchases, leaderboard queries). For high-frequency state that changes every
> tick, use entity replication instead — it is far more bandwidth-efficient
> thanks to per-field delta compression.

---

## Full working example

See `demos/bevy/shared/src/messages/basic_request.rs` for the type definitions
and `demos/bevy/server/src/systems/events.rs` + `demos/bevy/client/src/systems/events.rs`
for the complete send/receive pattern.
