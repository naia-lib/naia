# Bevy Adapter

The Bevy adapter (`naia-bevy-server`, `naia-bevy-client`) wraps naia's core
crates and exposes `Server` / `Client` as Bevy resources, routes naia events
into Bevy's event system, and provides `CommandsExt` extension methods for
entity replication.

---

## The `T` phantom type parameter

When using the Bevy adapter, the `Client` SystemParam and `NaiaClientPlugin`
carry a generic type parameter `T`:

```rust
use naia_bevy_client::{Client, NaiaClientPlugin};

#[derive(Resource)]
pub struct MyClient;

app.add_plugins(NaiaClientPlugin::<MyClient>::new(client_config, protocol()));

fn my_system(client: Client<MyClient>) { … }
```

**Why does `T` exist?**

Bevy applications sometimes run more than one naia client simultaneously (for
example, a split-screen game where each half is a separate session, or a relay
node that bridges two servers). The `T` phantom marker lets Bevy distinguish
the two `Client` SystemParams at compile time — they are different types and
therefore different Bevy resources, with no runtime overhead.

> **Tip:** For single-client apps use `DefaultClientTag` and `DefaultPlugin` to avoid
> the boilerplate entirely:

```rust
use naia_bevy_client::{DefaultPlugin, Client, DefaultClientTag};

app.add_plugins(DefaultPlugin::new(client_config, protocol()));

fn my_system(client: Client<DefaultClientTag>) { … }
```

---

## System ordering

naia's Bevy plugins register systems that must run in the correct order relative
to your game systems. The plugins add a `NaiaSystemSet` that you can use to
order your systems explicitly:

```rust
app.configure_sets(
    Update,
    (NaiaSystemSet::ReceiveEvents, MyGameSet::Logic, NaiaSystemSet::Send).chain(),
);
```

See `demos/bevy/` for a complete ordering example.

---

## Multi-client setup

For games with two simultaneous naia clients (split-screen, relay nodes):

```rust
#[derive(Resource)]
pub struct ClientA;

#[derive(Resource)]
pub struct ClientB;

app.add_plugins(NaiaClientPlugin::<ClientA>::new(config_a, protocol()))
   .add_plugins(NaiaClientPlugin::<ClientB>::new(config_b, protocol()));

fn system_a(client: Client<ClientA>) { … }
fn system_b(client: Client<ClientB>) { … }
```

Both clients are fully independent — separate connections, separate event
queues, separate entity sets.
