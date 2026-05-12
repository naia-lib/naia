# Request / Response

naia supports typed request/response pairs over reliable bidirectional channels.
This is useful for one-shot operations where the caller needs a reply: loading a
level, querying a leaderboard entry, or submitting an item purchase.

---

## Defining a request/response pair

```rust
#[derive(Request, Response)]
pub struct FetchScore {
    pub player_id: u32,
}

pub struct FetchScoreResponse {
    pub score: u32,
    pub rank: u32,
}
```

Register both types in the `Protocol`:

```rust
Protocol::builder()
    .add_request_response::<FetchScore, FetchScoreResponse>()
    .build()
```

---

## Sending a request (client)

```rust
let request_id = client.send_request::<FetchScore>(&FetchScore { player_id: 42 });
```

`send_request` returns a `RequestId` you can use to match the reply.

---

## Handling the request (server)

```rust
for (user_key, request_id, request) in events.read::<RequestEvent<FetchScore>>() {
    let score = lookup_score(request.player_id);
    server.send_response(&user_key, &request_id,
        &FetchScoreResponse { score, rank: 1 })?;
}
```

---

## Receiving the response (client)

```rust
for (request_id, response) in events.read::<ResponseEvent<FetchScoreResponse>>() {
    println!("Score: {}, Rank: {}", response.score, response.rank);
}
```

---

## TTL and disconnect cleanup

Pending requests are automatically cancelled when the connection drops. The
server-side request queue is bounded — if the client sends requests faster than
the server can respond, older requests are dropped.

> **Tip:** Use request/response for infrequent operations (level transitions, purchases,
> leaderboard queries). For high-frequency state, use entity replication instead.
