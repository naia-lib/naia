//! # Cyberlith Sync Engine – Overview
//!
//! **Mission Statement**
//! Keep a distributed **Entity‑Component System (ECS)** in tight,
//! synchrony over an *unordered, reliable* transport
//! **without incurring head‑of‑line blocking
//! (HoLB)** for unrelated entities or components.
//!
//! ## Architectural sketch
//! 1. **Message production**  
//!    *Sender* listens to ECS change‑events and emits `EntityMessage<E>` records,
//!    each tagged with a monotonically increasing `MessageIndex` (`u16`).
//! 2. **Transport**  
//!    Messages are batched into packets and delivered over an unordered
//!    reliability layer. Packet‑level ACKs let the sender garbage‑collect
//!    its sliding window without caring about intra‑packet order.
//! 3. **Ingestion path (this crate)**  
//!    *Receiver* deduplicates on `MessageIndex` and feeds messages into
//!    [`RemoteEngine::receive_message`].
//!    The `Engine` owns one **`EntityChannel`** per live entity; each
//!    `EntityChannel` owns:
//!    - an **`AuthChannel`** (publish / delegation / authority negotiation)  
//!    - many **`ComponentChannel`s** (insert / remove per component)
//!
//!    Each sub‑channel is an *independent state machine* that guarantees
//!    **idempotent, in‑order delivery per logical stream** while allowing
//!    global out‑of‑order arrival.  
//!    Once a channel determines that a message is *now safe* to apply, it
//!    is pushed into `outgoing_events`; the caller drains these via
//!    [`RemoteEngine::take_incoming_events`] and mutates its local ECS accordingly.
//!
//! ## Why unordered beats ordered
//! * Ordered transports serialize unrelated entity updates, so a single
//!   delayed packet stalls the **entire world** (classical HoLB).
//! * By partitioning the stream **per entity → per component/auth domain**,
//!   we localise ordering to the *minimum necessary scope*.
//!
//! ## Safety & correctness guarantees
//! * **At‑most‑once** delivery per `(MessageIndex, Entity)` pair.
//! * **Per‑entity causal ordering** for spawns, despawns, and component
//!   inserts/removes—achieved via wrap‑around‑safe sequence comparisons
//!   and channel‑local buffering.
//! * **Configurable guard‑band** (`EngineConfig::flush_threshold`) obliges the
//!   *sender* to flush before ID reuse; the *receiver* needs **no special logic**
//!   at the wrap because all comparisons are already wrap‑safe.
//!
//! ## Reading map
//! | Module | Role |
//! |--------|------|
//! | [`engine.rs`]          | top‑level orchestrator |
//! | [`config.rs`]          | compile‑time tuning knobs |
//! | [`entity_channel.rs`]  | per‑entity dispatcher |
//! | [`component_channel.rs`]| component add/remove FSM |
//! | [`auth_channel.rs`]    | authority & delegation FSM |
//!
//! Together they form a lock‑free, allocation‑conscious pipeline for syncing ECS worlds in distributed systems.

mod auth_channel_receiver;
mod config;
mod remote_component_channel;
mod remote_engine;
pub mod remote_entity_channel;

mod auth_channel_sender;
mod host_engine;
pub mod host_entity_channel;

pub use host_engine::HostEngine;
pub use host_entity_channel::HostEntityChannel;
pub use remote_engine::RemoteEngine;
pub use remote_entity_channel::RemoteEntityChannel;

pub mod auth_channel;
pub mod bevy_integration;
pub mod ordered_ids;
#[cfg(test)]
pub mod tests;
