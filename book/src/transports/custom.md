# Writing a Custom Transport

**Trait location:** `naia_shared::transport::Socket`

naia's transport is defined by the `Socket` trait. Implementing it lets you
plug in any network layer — Steam networking, QUIC, WebSockets, etc.

---

## The Socket trait

```rust
pub trait Socket: Send + Sync + 'static {
    fn send(&mut self, address: SocketAddr, payload: &[u8]);
    fn receive(&mut self) -> Option<(SocketAddr, Vec<u8>)>;
}
```

`send` delivers a payload to the given address. `receive` polls for the next
received datagram, returning `None` if no data is available.

---

## Example: Steam networking stub

```rust
pub struct SteamSocket {
    // Valve SDR networking handle
}

impl Socket for SteamSocket {
    fn send(&mut self, address: SocketAddr, payload: &[u8]) {
        // Translate SocketAddr to SteamNetworkingIdentity
        // Call ISteamNetworkingSockets::SendMessageToConnection
    }

    fn receive(&mut self) -> Option<(SocketAddr, Vec<u8>)> {
        // Poll ISteamNetworkingSockets::ReceiveMessagesOnConnection
        // Return None if queue is empty
    }
}
```

---

## Registration

Pass your socket implementation to `server.listen` / `client.connect` just like
the built-in transports:

```rust
server.listen(SteamSocket::new(connection_handle));
```

> **Note:** naia does not ship a Steam transport. The `Socket` trait is the extension point
> for community crates.
