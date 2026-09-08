use std::time::Duration;

use naia_socket_shared::{LinkConditionerConfig, SocketConfig};

use crate::{
    connection::compression_config::{CompressionConfig, CompressionMode},
    messages::{
        channels::{
            channel::{Channel, ChannelDirection, ChannelMode, ChannelSettings},
            channel_kinds::ChannelKinds,
            default_channels::DefaultChannelsPlugin,
        },
        fragment::FragmentedMessage,
        message::Message,
        message_kinds::MessageKinds,
    },
    protocol_id::ProtocolId,
    world::{
        component::{component_kinds::ComponentKinds, replicate::Replicate},
        resource::ResourceKinds,
    },
    Request, RequestOrResponse,
};

/// Extension point for registering channels, messages, and components into a `Protocol`.
pub trait ProtocolPlugin {
    /// Applies this plugin's registrations to `protocol`.
    fn build(&self, protocol: &mut Protocol);
}

/// Builder and configuration container for a naia protocol definition.
///
/// Collects channels, messages, components, and transport settings before being locked and passed to a server or client.
#[derive(Clone)]
pub struct Protocol {
    /// Registry of all channels registered in this protocol.
    pub channel_kinds: ChannelKinds,
    /// Registry of all message types registered in this protocol.
    pub message_kinds: MessageKinds,
    /// Registry of all replicated component types registered in this protocol.
    pub component_kinds: ComponentKinds,
    /// Marker table — which `ComponentKind`s are Replicated Resources.
    /// Receiver side checks this on `SpawnWithComponents` to populate
    /// its `ResourceRegistry`. See `_AGENTS/RESOURCES_PLAN.md`.
    pub resource_kinds: ResourceKinds,
    /// Used to configure the underlying socket
    pub socket: SocketConfig,
    /// The duration between each tick
    pub tick_interval: Duration,
    /// Configuration used to control compression parameters
    pub compression: Option<CompressionConfig>,
    /// Whether or not Client Authoritative Entities will be allowed
    pub client_authoritative_entities: bool,
    /// Cached protocol ID, computed when lock() is called
    cached_protocol_id: Option<ProtocolId>,
    locked: bool,
}

impl Default for Protocol {
    fn default() -> Self {
        let mut message_kinds = MessageKinds::new();
        message_kinds.add_message::<FragmentedMessage>();
        message_kinds.add_message::<RequestOrResponse>();

        let channel_kinds = ChannelKinds::new();

        Self {
            channel_kinds,
            message_kinds,
            component_kinds: ComponentKinds::new(),
            resource_kinds: ResourceKinds::new(),
            socket: SocketConfig::new(None, None),
            tick_interval: Duration::from_millis(50),
            compression: None,
            client_authoritative_entities: false,
            cached_protocol_id: None,
            locked: false,
        }
    }
}

impl Protocol {
    /// Returns a default `Protocol` ready for builder-style configuration.
    ///
    /// Mutation triage: replacing this body with `Default::default()` is an
    /// equivalent mutant -- that is exactly what it does. Named for readability
    /// at the call site, not for behavior.
    pub fn builder() -> Self {
        Self::default()
    }

    /// Applies `plugin`'s registrations to this protocol. Builder-style.
    pub fn add_plugin<P: ProtocolPlugin>(&mut self, plugin: P) -> &mut Self {
        self.check_lock();
        plugin.build(self);
        self
    }

    /// Sets the link conditioning configuration (artificial latency/loss). Builder-style.
    pub fn link_condition(&mut self, config: LinkConditionerConfig) -> &mut Self {
        self.check_lock();
        self.socket.link_condition = Some(config);
        self
    }

    /// Sets the WebRTC signalling endpoint path. Builder-style.
    pub fn rtc_endpoint(&mut self, path: String) -> &mut Self {
        self.check_lock();
        self.socket.rtc_endpoint_path = path;
        self
    }

    /// Returns the configured WebRTC signalling endpoint path.
    pub fn get_rtc_endpoint(&self) -> String {
        self.socket.rtc_endpoint_path.clone()
    }

    /// Sets the server tick interval. Builder-style.
    pub fn tick_interval(&mut self, duration: Duration) -> &mut Self {
        self.check_lock();
        self.tick_interval = duration;
        self
    }

    /// Enables packet compression with the given config. Builder-style.
    pub fn compression(&mut self, config: CompressionConfig) -> &mut Self {
        self.check_lock();
        self.compression = Some(config);
        self
    }

    /// Enables client-authoritative entity mode, allowing clients to own and update replicated entities. Builder-style.
    pub fn enable_client_authoritative_entities(&mut self) -> &mut Self {
        self.check_lock();
        self.client_authoritative_entities = true;
        self
    }

    /// Registers the six built-in default channels. Builder-style.
    pub fn add_default_channels(&mut self) -> &mut Self {
        self.check_lock();
        let plugin = DefaultChannelsPlugin;
        plugin.build(self);
        self
    }

    /// Registers channel type `C` with the given direction and mode. Builder-style.
    pub fn add_channel<C: Channel>(
        &mut self,
        direction: ChannelDirection,
        mode: ChannelMode,
    ) -> &mut Self {
        self.check_lock();
        self.channel_kinds
            .add_channel::<C>(ChannelSettings::new(mode, direction));
        self
    }

    /// Register a channel with fully-specified `ChannelSettings` (including
    /// `criticality`). Use this when you need a non-default priority tier;
    /// otherwise `add_channel` is sufficient.
    pub fn add_channel_settings<C: Channel>(&mut self, settings: ChannelSettings) -> &mut Self {
        self.check_lock();
        self.channel_kinds.add_channel::<C>(settings);
        self
    }

    /// Registers message type `M`. Builder-style.
    pub fn add_message<M: Message>(&mut self) -> &mut Self {
        self.check_lock();
        self.message_kinds.add_message::<M>();
        self
    }

    /// Registers request type `Q` and its associated response type. Builder-style.
    pub fn add_request<Q: Request>(&mut self) -> &mut Self {
        self.check_lock();
        // Requests and Responses are handled just like Messages
        self.message_kinds.add_message::<Q>();
        self.message_kinds.add_message::<Q::Response>();
        self
    }

    /// Registers replicated component type `C`. Builder-style.
    pub fn add_component<C: Replicate>(&mut self) -> &mut Self {
        self.check_lock();
        self.component_kinds.add_component::<C>();
        self
    }

    /// Register `R` as a Replicated Resource.
    ///
    /// A Resource is internally a hidden 1-component entity carrying `R`
    /// as its sole replicated component. This call:
    ///
    /// 1. Calls `add_component::<R>()` to allocate a normal `ComponentKind`
    ///    + NetId for `R` (Resources reuse the component wire encoding).
    /// 2. Records the `ComponentKind` in `resource_kinds` so the receiver
    ///    side can recognize incoming SpawnWithComponents messages whose
    ///    components are resources, and populate its `ResourceRegistry`.
    ///
    /// Idempotent — registering the same type twice is a no-op (matches
    /// `add_component` re-registration semantics; the underlying tables
    /// dedupe on `TypeId`).
    pub fn add_resource<R: Replicate>(&mut self) -> &mut Self {
        self.check_lock();
        // Allocate a ComponentKind for R if not already present.
        self.component_kinds.add_component::<R>();
        // Mark the kind as a resource.
        let kind = crate::ComponentKind::of::<R>();
        self.resource_kinds.register::<R>(kind);
        self
    }

    /// Freezes the protocol, computes and caches the protocol ID. Must be called before use.
    pub fn lock(&mut self) {
        self.check_lock();
        self.cached_protocol_id = Some(self.compute_protocol_id());
        self.locked = true;
    }

    /// Locks the protocol if it is not already locked, then returns its
    /// fingerprint.
    ///
    /// Constructors that need the fingerprint cannot just call
    /// [`lock`](Self::lock): the `Protocol` handed to them may already be
    /// locked — `Server::new` locks once and then clones the locked protocol
    /// into both the main server and the world server — and `lock` panics on a
    /// second call. This is the form to use anywhere the lock state is not
    /// known statically.
    pub fn locked_protocol_id(&mut self) -> ProtocolId {
        if !self.locked {
            self.lock();
        }
        self.protocol_id()
    }

    /// Panics if the protocol has already been locked.
    pub fn check_lock(&self) {
        if self.locked {
            panic!("Protocol already locked!");
        }
    }

    /// Moves out of the builder and returns the owned `Protocol`.
    pub fn build(&mut self) -> Self {
        std::mem::take(self)
    }

    /// Returns the cached protocol ID. Panics if protocol is not locked.
    pub fn protocol_id(&self) -> ProtocolId {
        self.cached_protocol_id
            .expect("Protocol must be locked before calling protocol_id()")
    }

    /// The resource section's input: every registered resource resolved to its
    /// component name, sorted.
    ///
    /// `ResourceKinds` is a `HashSet`, so its iteration order is not even
    /// stable within one process — the sort is what makes the section a
    /// function of the *membership* rather than of the traversal. It is a named
    /// method rather than an inline block so the sort can be asserted directly:
    /// two protocols with the same members hash the same whether it is there or
    /// not, so equality alone cannot catch its removal.
    fn resource_schema_names(&self) -> Vec<String> {
        let mut names: Vec<String> = self
            .resource_kinds
            .iter()
            .map(|kind| self.component_kinds.kind_to_name(kind))
            .collect();
        names.sort();
        names
    }

    /// Compute the protocol fingerprint from current state.
    ///
    /// See [`PROTOCOL_FINGERPRINT_FORMAT`] for the preimage grammar and the
    /// rules that keep it honest.
    fn compute_protocol_id(&self) -> ProtocolId {
        let mut hasher = blake3::Hasher::new();

        // Format tag. Separates this grammar from any future one, so an
        // encoding change and a schema change can never be confused.
        hasher.update(PROTOCOL_FINGERPRINT_FORMAT);

        // Channels, in wire net-ID order, each with the complete settings
        // encoding — mode *with its payload*, direction, criticality.
        let channels = self.channel_kinds.schema_entries();
        hasher.update(SECTION_CHANNELS);
        update_count(&mut hasher, channels.len());
        for (net_id, name, settings) in &channels {
            hasher.update(&net_id.to_le_bytes());
            update_bytes(&mut hasher, name.as_bytes());
            update_bytes(&mut hasher, settings);
        }

        // Messages, in wire net-ID order.
        let messages = self.message_kinds.schema_entries();
        hasher.update(SECTION_MESSAGES);
        update_count(&mut hasher, messages.len());
        for (net_id, name) in &messages {
            hasher.update(&net_id.to_le_bytes());
            update_bytes(&mut hasher, name.as_bytes());
        }

        // Components, in wire net-ID order.
        let components = self.component_kinds.schema_entries();
        hasher.update(SECTION_COMPONENTS);
        update_count(&mut hasher, components.len());
        for (net_id, name) in &components {
            hasher.update(&net_id.to_le_bytes());
            update_bytes(&mut hasher, name.as_bytes());
        }

        // Resources are a *set*: membership carries no wire ordinal of its
        // own, so the members are resolved to their component names and
        // sorted. The previous encoding folded in only a count, which meant
        // that swapping which of two kinds was the resource — the common
        // case, and one that changes how the receiver populates its
        // ResourceRegistry — produced an identical id.
        let resources = self.resource_schema_names();
        hasher.update(SECTION_RESOURCES);
        update_count(&mut hasher, resources.len());
        for name in &resources {
            update_bytes(&mut hasher, name.as_bytes());
        }

        // Compression. Whether a direction is compressed, and with exactly
        // which parameters, decides whether the far end can read a packet at
        // all — a peer that decompresses with a different dictionary gets
        // garbage, not a clean refusal. Both directions are always written, in
        // a fixed order, so "compressed one way" and "compressed the other
        // way" cannot collide.
        hasher.update(SECTION_COMPRESSION);
        match &self.compression {
            None => {
                hasher.update(&[0u8]);
            }
            Some(config) => {
                hasher.update(&[1u8]);
                update_compression_mode(&mut hasher, config.server_to_client.as_ref());
                update_compression_mode(&mut hasher, config.client_to_server.as_ref());
            }
        }

        // Runtime modes that change what a peer is allowed to put on the wire.
        hasher.update(SECTION_RUNTIME);
        hasher.update(&[u8::from(self.client_authoritative_entities)]);

        // Codec grammar. Bumped when naia's own encoding of frames, headers or
        // net-ID fields changes — never per message and never by an
        // application.
        hasher.update(SECTION_CODEC);
        hasher.update(&CODEC_GRAMMAR_VERSION.to_le_bytes());

        let hash = hasher.finalize();
        let mut bytes = [0u8; ProtocolId::BYTE_LEN];
        bytes.copy_from_slice(&hash.as_bytes()[..ProtocolId::BYTE_LEN]);
        ProtocolId::from_bytes(bytes)
    }
}

/// Domain-separation tag for the protocol fingerprint preimage.
///
/// Bump the trailing version whenever the *grammar below* changes — the order
/// of sections, the framing of an item, the set of facts folded in. Bumping it
/// changes every protocol's fingerprint, which is correct: peers built against
/// two different grammars have not actually agreed on anything and must not
/// connect on a coincidence.
///
/// # Grammar
///
/// ```text
/// "naia:pf:v1"
/// "\x01chan"  count:u32  then per channel IN NET-ID ORDER:
///               net_id:u16 LE, len:u32 LE + name bytes,
///               len:u32 LE + ChannelSettings::schema_bytes:
///                 mode:u8
///                 + UnorderedReliable|SequencedReliable|OrderedReliable:
///                     rtt_resend_factor:f32 bits LE,
///                     max_queue_depth: 0x00 | 0x01 + u64 LE
///                 + TickBuffered: message_capacity:u64 LE
///                 + unreliable modes: nothing
///                 direction:u8, criticality:u8
/// "\x02msg"   count:u32  then per message IN NET-ID ORDER:
///               net_id:u16 LE, len:u32 LE + name bytes
/// "\x03comp"  count:u32  then per component IN NET-ID ORDER:
///               net_id:u16 LE, len:u32 LE + name bytes
/// "\x04res"   count:u32  then resource component-names SORTED:
///               len:u32 LE + name bytes
/// "\x06comp2" 0x00 (no compression) | 0x01 then, server->client first
///             and client->server second, per direction:
///               0x00 (uncompressed) | 0x01 + mode:u8:
///                 0 Default:    level:i32 LE
///                 1 Dictionary: level:i32 LE, len:u32 LE + blake3(dict)
///                 2 Training:   samples:u64 LE
/// "\x07rt"    client_authoritative_entities:u8
/// "\x05codec" codec_grammar_version:u32 LE
/// ```
///
/// Sections are written in the order shown by the code, which is the order of
/// the block above; the tag bytes are unique identifiers, not a sort key, so
/// `\x06`/`\x07` preceding `\x05` here is deliberate — appending new sections
/// before the codec tag keeps the codec version last, where it reads as the
/// closing statement about naia's own encoding.
///
/// Deployment-only facts are excluded on purpose: socket addresses, the RTC
/// endpoint path, link-conditioning simulation and tick interval do not change
/// how a byte on the wire is decoded, and folding them in would refuse
/// perfectly compatible peers.
///
/// Three properties are load-bearing, and each exists because its absence was
/// a real hole in the previous encoding:
///
/// - **Net-ID order, with the id written out.** Net-IDs are registration
///   ordinals and travel on the wire. The previous encoding hashed *sorted
///   names*, so reordering two registrations renumbered every kind on the wire
///   and left the id bit-identical.
/// - **Length prefixes on every name and settings blob.** Raw concatenation
///   made `["AB", "C"]` and `["A", "BC"]` hash the same.
/// - **Section tags with counts.** Without them the three name groups ran
///   together, so a name moving from the message group to the component group
///   was invisible.
///
/// # What is deliberately absent
///
/// Field order, wire types and widths inside a struct, enum variant order and
/// discriminants, component property schema, and request→response pairing are
/// **not** covered. None of them survives macro expansion into any runtime
/// value, so covering them requires the derives to emit schema descriptors —
/// separate work. Do not read a matching fingerprint as agreement on field
/// layout.
pub const PROTOCOL_FINGERPRINT_FORMAT: &[u8] = b"naia:pf:v1";

const SECTION_CHANNELS: &[u8] = b"\x01chan";
const SECTION_MESSAGES: &[u8] = b"\x02msg";
const SECTION_COMPONENTS: &[u8] = b"\x03comp";
const SECTION_RESOURCES: &[u8] = b"\x04res";
const SECTION_CODEC: &[u8] = b"\x05codec";
const SECTION_COMPRESSION: &[u8] = b"\x06comp2";
const SECTION_RUNTIME: &[u8] = b"\x07rt";

/// Fold one direction's compression setting into the preimage.
///
/// `0x00` for "this direction is not compressed"; otherwise `0x01`, the mode
/// discriminant, and the mode's full parameters. The discriminants are
/// hand-pinned here for the same reason as the channel ones: a variant reorder
/// in `CompressionMode` must not silently move them.
///
/// A custom dictionary is folded in as a BLAKE3 digest of its bytes rather
/// than the bytes themselves. The digest is what the fingerprint needs — two
/// peers must have the *same* dictionary, and a digest settles that — and it
/// keeps a multi-megabyte dictionary from being rehashed on every
/// `protocol_id()` call. It is length-prefixed like every other byte string,
/// so a digest can never run together with what follows.
fn update_compression_mode(hasher: &mut blake3::Hasher, mode: Option<&CompressionMode>) {
    let Some(mode) = mode else {
        hasher.update(&[0u8]);
        return;
    };
    hasher.update(&[1u8]);
    match mode {
        CompressionMode::Default(level) => {
            hasher.update(&[0u8]);
            hasher.update(&level.to_le_bytes());
        }
        CompressionMode::Dictionary(level, dictionary) => {
            hasher.update(&[1u8]);
            hasher.update(&level.to_le_bytes());
            update_bytes(hasher, blake3::hash(dictionary).as_bytes());
        }
        CompressionMode::Training(samples) => {
            hasher.update(&[2u8]);
            hasher.update(&(*samples as u64).to_le_bytes());
        }
    }
}

/// Version of naia's own codec grammar: frame and header layout, and the
/// `ceil(log2(count))` net-ID bit-field encoding.
///
/// This is a property of the naia implementation, not of any application
/// protocol. Bump it when the encoding changes; never expose it to consumers
/// as a per-message or per-application version.
pub const CODEC_GRAMMAR_VERSION: u32 = 1;

/// Fold a length-prefixed byte string into the preimage.
///
/// The prefix is what stops two different name lists from producing the same
/// concatenation.
fn update_bytes(hasher: &mut blake3::Hasher, bytes: &[u8]) {
    hasher.update(&(bytes.len() as u32).to_le_bytes());
    hasher.update(bytes);
}

/// Fold a section item count into the preimage.
///
/// Counts are their own fact, not just a redundant check: the registries
/// derive `kind_bit_width` as `ceil(log2(count))` and read it on the hot path,
/// so crossing a power-of-two boundary reframes every net-ID field on the
/// wire.
fn update_count(hasher: &mut blake3::Hasher, count: usize) {
    hasher.update(&(count as u32).to_le_bytes());
}

#[cfg(test)]
mod protocol_tests {
    use std::time::Duration;

    use naia_socket_shared::LinkConditionerConfig;

    use crate::{
        connection::compression_config::{CompressionConfig, CompressionMode},
        ChannelCriticality, ComponentKind, Message, Property, ReliableSettings, Replicate, Request,
        Response, TickBufferSettings,
    };

    use super::{ChannelDirection, ChannelMode, ChannelSettings, Protocol, ProtocolPlugin};

    macro_rules! test_channel {
        ($name:ident) => {
            struct $name;
            impl crate::Named for $name {
                fn name(&self) -> String {
                    stringify!($name).to_string()
                }
                fn protocol_name() -> &'static str {
                    stringify!($name)
                }
            }
            impl crate::Channel for $name {}
        };
    }

    test_channel!(Gossip);
    test_channel!(Rumor);

    #[derive(Message)]
    struct Whisper {
        value: u8,
    }

    #[derive(Message)]
    struct Shout {
        value: u8,
    }

    #[derive(Message)]
    struct Question {
        value: u8,
    }

    #[derive(Message)]
    struct Answer {
        value: u8,
    }

    impl Request for Question {
        type Response = Answer;
    }
    impl Response for Answer {}

    #[derive(Replicate)]
    struct Ghost {
        value: Property<u8>,
    }

    #[derive(Replicate)]
    struct Wraith {
        value: Property<u8>,
    }

    fn settings() -> ChannelSettings {
        ChannelSettings::new(
            ChannelMode::UnorderedUnreliable,
            ChannelDirection::Bidirectional,
        )
    }

    #[test]
    fn a_fresh_protocol_carries_the_two_built_in_messages_and_nothing_else() {
        let protocol = Protocol::builder();

        assert_eq!(protocol.message_kinds.all_names().len(), 2);
        assert!(protocol.channel_kinds.all_names().is_empty());
        assert!(protocol.component_kinds.all_names().is_empty());
        assert!(protocol.resource_kinds.is_empty());
        assert_eq!(protocol.tick_interval, Duration::from_millis(50));
        assert!(protocol.compression.is_none());
        assert!(!protocol.client_authoritative_entities);
    }

    #[test]
    fn every_setter_records_its_value_and_hands_the_builder_back() {
        let mut protocol = Protocol::builder();
        protocol
            .link_condition(LinkConditionerConfig::good_condition())
            .rtc_endpoint("/rtc".to_string())
            .tick_interval(Duration::from_millis(20))
            .compression(CompressionConfig::new(None, None))
            .enable_client_authoritative_entities();

        assert!(protocol.socket.link_condition.is_some());
        assert_eq!(protocol.get_rtc_endpoint(), "/rtc".to_string());
        assert_eq!(protocol.tick_interval, Duration::from_millis(20));
        assert!(protocol.compression.is_some());
        assert!(protocol.client_authoritative_entities);
    }

    #[test]
    fn each_registry_takes_what_its_own_method_registers() {
        let mut protocol = Protocol::builder();
        protocol
            .add_channel::<Gossip>(
                ChannelDirection::Bidirectional,
                ChannelMode::UnorderedUnreliable,
            )
            .add_channel_settings::<Rumor>(settings())
            .add_message::<Whisper>()
            .add_component::<Ghost>();

        assert_eq!(
            protocol.channel_kinds.all_names(),
            vec!["Gossip".to_string(), "Rumor".to_string()]
        );
        assert!(protocol
            .message_kinds
            .all_names()
            .contains(&"Whisper".to_string()));
        assert_eq!(
            protocol.component_kinds.all_names(),
            vec!["Ghost".to_string()]
        );
    }

    #[test]
    fn the_six_default_channels_arrive_together() {
        let mut protocol = Protocol::builder();
        protocol.add_default_channels();

        assert_eq!(protocol.channel_kinds.all_names().len(), 6);
    }

    #[test]
    fn a_request_registers_both_halves_of_the_exchange() {
        let mut protocol = Protocol::builder();
        protocol.add_request::<Question>();

        let names = protocol.message_kinds.all_names();
        assert!(names.contains(&"Question".to_string()));
        assert!(names.contains(&"Answer".to_string()));
    }

    #[test]
    fn a_resource_is_a_component_that_is_also_marked() {
        let mut protocol = Protocol::builder();
        protocol.add_resource::<Ghost>();

        let kind = ComponentKind::of::<Ghost>();
        assert_eq!(
            protocol.component_kinds.all_names(),
            vec!["Ghost".to_string()]
        );
        assert!(protocol.resource_kinds.is_resource(&kind));
        assert_eq!(protocol.resource_kinds.kind_for::<Ghost>(), Some(kind));
    }

    #[test]
    fn registering_the_same_resource_twice_changes_nothing() {
        let mut protocol = Protocol::builder();
        protocol.add_resource::<Ghost>().add_resource::<Ghost>();

        assert_eq!(protocol.component_kinds.all_names().len(), 1);
        assert_eq!(protocol.resource_kinds.len(), 1);
    }

    struct Furnishings;

    impl ProtocolPlugin for Furnishings {
        fn build(&self, protocol: &mut Protocol) {
            protocol.add_message::<Whisper>().add_component::<Ghost>();
        }
    }

    #[test]
    fn a_plugin_registers_through_the_protocol_it_is_handed() {
        let mut protocol = Protocol::builder();
        protocol.add_plugin(Furnishings);

        assert!(protocol
            .message_kinds
            .all_names()
            .contains(&"Whisper".to_string()));
        assert_eq!(
            protocol.component_kinds.all_names(),
            vec!["Ghost".to_string()]
        );
    }

    #[test]
    fn building_moves_the_registrations_out_and_leaves_a_default_behind() {
        let mut builder = Protocol::builder();
        builder.add_message::<Whisper>();
        let built = builder.build();

        assert!(built
            .message_kinds
            .all_names()
            .contains(&"Whisper".to_string()));
        assert!(!builder
            .message_kinds
            .all_names()
            .contains(&"Whisper".to_string()));
        assert_eq!(builder.message_kinds.all_names().len(), 2);
    }

    fn locked(configure: impl FnOnce(&mut Protocol)) -> Protocol {
        let mut protocol = Protocol::builder();
        configure(&mut protocol);
        protocol.lock();
        protocol
    }

    #[test]
    fn the_protocol_id_is_unavailable_until_the_protocol_is_locked() {
        let protocol = Protocol::builder();
        let panicked =
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| protocol.protocol_id()));
        assert!(panicked.is_err());
    }

    #[test]
    fn locking_caches_an_id_that_stays_the_same_on_every_read() {
        let protocol = locked(|p| {
            p.add_message::<Whisper>();
        });

        assert_eq!(protocol.protocol_id(), protocol.protocol_id());
    }

    #[test]
    fn two_protocols_registering_the_same_names_agree_on_an_id() {
        let build = || {
            locked(|p| {
                p.add_channel::<Gossip>(
                    ChannelDirection::Bidirectional,
                    ChannelMode::UnorderedUnreliable,
                );
                p.add_message::<Whisper>();
                p.add_component::<Ghost>();
            })
        };

        assert_eq!(build().protocol_id(), build().protocol_id());
    }

    #[test]
    fn a_difference_in_any_registry_is_a_difference_in_the_id() {
        let base = locked(|p| {
            p.add_channel::<Gossip>(
                ChannelDirection::Bidirectional,
                ChannelMode::UnorderedUnreliable,
            );
            p.add_message::<Whisper>();
            p.add_component::<Ghost>();
        });

        let other_channel = locked(|p| {
            p.add_channel::<Rumor>(
                ChannelDirection::Bidirectional,
                ChannelMode::UnorderedUnreliable,
            );
            p.add_message::<Whisper>();
            p.add_component::<Ghost>();
        });
        let other_message = locked(|p| {
            p.add_channel::<Gossip>(
                ChannelDirection::Bidirectional,
                ChannelMode::UnorderedUnreliable,
            );
            p.add_message::<Shout>();
            p.add_component::<Ghost>();
        });
        let other_component = locked(|p| {
            p.add_channel::<Gossip>(
                ChannelDirection::Bidirectional,
                ChannelMode::UnorderedUnreliable,
            );
            p.add_message::<Whisper>();
            p.add_component::<Wraith>();
        });

        assert_ne!(base.protocol_id(), other_channel.protocol_id());
        assert_ne!(base.protocol_id(), other_message.protocol_id());
        assert_ne!(base.protocol_id(), other_component.protocol_id());
    }

    #[test]
    fn the_same_type_hashes_differently_as_a_resource_than_as_a_component() {
        let as_component = locked(|p| {
            p.add_component::<Ghost>();
        });
        let as_resource = locked(|p| {
            p.add_resource::<Ghost>();
        });

        assert_ne!(as_component.protocol_id(), as_resource.protocol_id());
    }

    #[test]
    fn every_builder_method_refuses_to_run_once_the_protocol_is_locked() {
        macro_rules! assert_refused {
            ($body:expr) => {{
                let mut protocol = Protocol::builder();
                protocol.lock();
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    let protocol = &mut protocol;
                    $body(protocol);
                }));
                assert!(result.is_err());
            }};
        }

        let quiet = std::panic::take_hook();
        std::panic::set_hook(Box::new(|_| {}));

        assert_refused!(|p: &mut Protocol| {
            p.add_plugin(Furnishings);
        });
        assert_refused!(|p: &mut Protocol| {
            p.link_condition(LinkConditionerConfig::good_condition());
        });
        assert_refused!(|p: &mut Protocol| {
            p.rtc_endpoint("/rtc".to_string());
        });
        assert_refused!(|p: &mut Protocol| {
            p.tick_interval(Duration::from_millis(1));
        });
        assert_refused!(|p: &mut Protocol| {
            p.compression(CompressionConfig::new(None, None));
        });
        assert_refused!(|p: &mut Protocol| {
            p.enable_client_authoritative_entities();
        });
        assert_refused!(|p: &mut Protocol| {
            p.add_default_channels();
        });
        assert_refused!(|p: &mut Protocol| {
            p.add_channel::<Gossip>(
                ChannelDirection::Bidirectional,
                ChannelMode::UnorderedUnreliable,
            );
        });
        assert_refused!(|p: &mut Protocol| {
            p.add_channel_settings::<Gossip>(settings());
        });
        assert_refused!(|p: &mut Protocol| {
            p.add_message::<Whisper>();
        });
        assert_refused!(|p: &mut Protocol| {
            p.add_request::<Question>();
        });
        assert_refused!(|p: &mut Protocol| {
            p.add_component::<Ghost>();
        });
        assert_refused!(|p: &mut Protocol| {
            p.add_resource::<Ghost>();
        });
        assert_refused!(|p: &mut Protocol| {
            p.lock();
        });

        std::panic::set_hook(quiet);
    }

    // ---- protocol fingerprint oracles --------------------------------------
    //
    // Each of these pins one property that the fingerprint has to have for the
    // handshake comparison to mean anything. They are grouped here rather than
    // scattered so that a change to `compute_protocol_id` reds a legible set.

    /// F1 -- **Registration order is part of the protocol.**
    ///
    /// Channels, messages and components are addressed on the wire by net-ID,
    /// and net-IDs are assigned in registration order. Two peers that register
    /// the same set in a different order will therefore disagree about what
    /// every ID *means* while agreeing about which types exist. This is the
    /// single most common way a protocol silently diverges (someone reorders
    /// two lines in a shared registration function), and the old order-blind
    /// id could not see it at all.
    #[test]
    fn the_same_registrations_in_the_opposite_order_are_a_different_protocol() {
        let one_way = locked(|p| {
            p.add_message::<Whisper>();
            p.add_message::<Shout>();
        });
        let other_way = locked(|p| {
            p.add_message::<Shout>();
            p.add_message::<Whisper>();
        });
        assert_ne!(one_way.protocol_id(), other_way.protocol_id());

        let one_way = locked(|p| {
            p.add_component::<Ghost>();
            p.add_component::<Wraith>();
        });
        let other_way = locked(|p| {
            p.add_component::<Wraith>();
            p.add_component::<Ghost>();
        });
        assert_ne!(one_way.protocol_id(), other_way.protocol_id());

        let one_way = locked(|p| {
            p.add_channel::<Gossip>(
                ChannelDirection::Bidirectional,
                ChannelMode::UnorderedUnreliable,
            );
            p.add_channel::<Rumor>(
                ChannelDirection::Bidirectional,
                ChannelMode::UnorderedUnreliable,
            );
        });
        let other_way = locked(|p| {
            p.add_channel::<Rumor>(
                ChannelDirection::Bidirectional,
                ChannelMode::UnorderedUnreliable,
            );
            p.add_channel::<Gossip>(
                ChannelDirection::Bidirectional,
                ChannelMode::UnorderedUnreliable,
            );
        });
        assert_ne!(one_way.protocol_id(), other_way.protocol_id());
    }

    /// F2a -- **Names cannot run together.**
    ///
    /// Without a length prefix, the two-name list `["AB", "C"]` and the
    /// two-name list `["A", "BC"]` hash identically: the preimage is the same
    /// four bytes either way. The prefix is what makes the encoding injective,
    /// so it is asserted directly rather than inferred from a pair of
    /// protocols that happen not to collide.
    #[test]
    fn length_prefixing_makes_adjacent_names_unambiguous() {
        fn digest(names: &[&str]) -> [u8; 32] {
            let mut hasher = blake3::Hasher::new();
            for name in names {
                super::update_bytes(&mut hasher, name.as_bytes());
            }
            *hasher.finalize().as_bytes()
        }

        assert_ne!(digest(&["AB", "C"]), digest(&["A", "BC"]));
        assert_ne!(digest(&["", "AB"]), digest(&["AB", ""]));
        // Sanity: identical input still agrees, so the assertions above are
        // about ambiguity and not about the helper being nondeterministic.
        assert_eq!(digest(&["AB", "C"]), digest(&["AB", "C"]));
    }

    /// F2b -- **Sections cannot borrow each other's entries.**
    ///
    /// Each section writes its own tag and its own count before its items. Two
    /// protocols that move one registration across a section boundary — the
    /// same name, a different kind of thing — must not collide.
    #[test]
    fn section_tags_and_counts_keep_the_registries_apart() {
        let as_message = locked(|p| {
            p.add_message::<Whisper>();
        });
        let as_component = locked(|p| {
            p.add_component::<Ghost>();
        });
        let neither = locked(|_| {});

        assert_ne!(as_message.protocol_id(), as_component.protocol_id());
        assert_ne!(as_message.protocol_id(), neither.protocol_id());
        assert_ne!(as_component.protocol_id(), neither.protocol_id());
    }

    /// F3 -- **Which types are resources is part of the protocol.**
    ///
    /// A resource is replicated as a singleton and lands in the receiver's
    /// `ResourceRegistry`; a plain component does not. The pre-repair encoding
    /// folded in only the resource *count*, so swapping which of two
    /// registered kinds was the resource — same count, different meaning —
    /// produced an identical id.
    #[test]
    fn swapping_which_registered_kind_is_the_resource_changes_the_id() {
        let ghost_is_the_resource = locked(|p| {
            p.add_resource::<Ghost>();
            p.add_component::<Wraith>();
        });
        let wraith_is_the_resource = locked(|p| {
            p.add_component::<Ghost>();
            p.add_resource::<Wraith>();
        });

        assert_ne!(
            ghost_is_the_resource.protocol_id(),
            wraith_is_the_resource.protocol_id(),
            "a count-only resource encoding would call these equal"
        );
    }

    /// F3b -- **Resource membership is a set, so its own order is not.**
    ///
    /// Resources carry no wire ordinal of their own, so the encoding sorts
    /// them. Two protocols that marked the same members in the other order must
    /// therefore agree — otherwise the fingerprint would refuse peers that are
    /// in fact compatible.
    ///
    /// The marking is done straight on `resource_kinds` rather than through
    /// `add_resource`, for two reasons. `add_resource` also allocates a
    /// *component* net-ID, and component order is order-sensitive by design
    /// (F1), so going through it would vary two sections at once and prove
    /// nothing about either. And re-registering an already-registered component
    /// does not dedupe — `ComponentKinds::add_component` appends a second
    /// net-ID for the same kind — so the obvious "register the components
    /// first, then mark them" shape does not hold the component section fixed
    /// either. That is a pre-existing defect, reported and deliberately not
    /// repaired here; it is not this fingerprint's to fix.
    ///
    /// Equality alone cannot catch a dropped sort — `resource_kinds` is a
    /// `HashSet`, and two sets with the same members happen to traverse the
    /// same way — so the sort is asserted directly on the section's input as
    /// well.
    #[test]
    fn resource_membership_does_not_depend_on_the_order_it_was_marked_in() {
        let with_resources_marked = |mark: fn(&mut Protocol)| {
            locked(|p| {
                p.add_component::<Ghost>();
                p.add_component::<Wraith>();
                mark(p);
            })
        };
        let one_way = with_resources_marked(|p| {
            p.resource_kinds
                .register::<Ghost>(ComponentKind::of::<Ghost>());
            p.resource_kinds
                .register::<Wraith>(ComponentKind::of::<Wraith>());
        });
        let other_way = with_resources_marked(|p| {
            p.resource_kinds
                .register::<Wraith>(ComponentKind::of::<Wraith>());
            p.resource_kinds
                .register::<Ghost>(ComponentKind::of::<Ghost>());
        });

        assert_eq!(one_way.protocol_id(), other_way.protocol_id());

        // The sort itself, on the vector the section is built from.
        assert_eq!(other_way.resource_schema_names(), ["Ghost", "Wraith"]);
    }

    /// Helper: two protocols identical but for one channel's settings.
    fn with_channel_settings(settings: ChannelSettings) -> Protocol {
        locked(|p| {
            p.add_channel_settings::<Gossip>(settings);
        })
    }

    fn reliable(rtt_resend_factor: f32, max_queue_depth: Option<usize>) -> ReliableSettings {
        ReliableSettings {
            rtt_resend_factor,
            max_queue_depth,
        }
    }

    /// F4 -- **Every wire-relevant channel setting is in the fingerprint.**
    ///
    /// Not just the mode's outer variant: its payload too. A peer whose
    /// reliable channel has a different receive window (`max_queue_depth`
    /// doubles as the window) will drop indices its partner considers in
    /// range; a peer whose tick buffer holds a different number of messages
    /// prunes at a different point. Both are silent divergence, and a
    /// fingerprint that hashed only the discriminant would call them equal.
    ///
    /// Deployment-only knobs — socket addresses, link-conditioning simulation
    /// — are deliberately *not* here; see the companion test below.
    #[test]
    fn a_change_to_any_wire_relevant_channel_setting_changes_the_id() {
        let base = with_channel_settings(ChannelSettings::new(
            ChannelMode::OrderedReliable(reliable(1.5, Some(1024))),
            ChannelDirection::Bidirectional,
        ));

        // --- mode payload: reliable settings ---
        assert_ne!(
            base.protocol_id(),
            with_channel_settings(ChannelSettings::new(
                ChannelMode::OrderedReliable(reliable(2.0, Some(1024))),
                ChannelDirection::Bidirectional,
            ))
            .protocol_id(),
            "rtt_resend_factor must be covered"
        );
        assert_ne!(
            base.protocol_id(),
            with_channel_settings(ChannelSettings::new(
                ChannelMode::OrderedReliable(reliable(1.5, Some(2048))),
                ChannelDirection::Bidirectional,
            ))
            .protocol_id(),
            "max_queue_depth value must be covered"
        );
        assert_ne!(
            base.protocol_id(),
            with_channel_settings(ChannelSettings::new(
                ChannelMode::OrderedReliable(reliable(1.5, None)),
                ChannelDirection::Bidirectional,
            ))
            .protocol_id(),
            "max_queue_depth None vs Some must be covered"
        );

        // --- mode discriminant, holding the payload constant ---
        assert_ne!(
            base.protocol_id(),
            with_channel_settings(ChannelSettings::new(
                ChannelMode::UnorderedReliable(reliable(1.5, Some(1024))),
                ChannelDirection::Bidirectional,
            ))
            .protocol_id(),
            "the mode variant itself must be covered"
        );
        assert_ne!(
            base.protocol_id(),
            with_channel_settings(ChannelSettings::new(
                ChannelMode::SequencedReliable(reliable(1.5, Some(1024))),
                ChannelDirection::Bidirectional,
            ))
            .protocol_id(),
        );

        // --- direction ---
        assert_ne!(
            base.protocol_id(),
            with_channel_settings(ChannelSettings::new(
                ChannelMode::OrderedReliable(reliable(1.5, Some(1024))),
                ChannelDirection::ClientToServer,
            ))
            .protocol_id(),
            "direction must be covered"
        );
        assert_ne!(
            with_channel_settings(ChannelSettings::new(
                ChannelMode::OrderedReliable(reliable(1.5, Some(1024))),
                ChannelDirection::ClientToServer,
            ))
            .protocol_id(),
            with_channel_settings(ChannelSettings::new(
                ChannelMode::OrderedReliable(reliable(1.5, Some(1024))),
                ChannelDirection::ServerToClient,
            ))
            .protocol_id(),
        );

        // --- criticality ---
        assert_ne!(
            base.protocol_id(),
            with_channel_settings(
                ChannelSettings::new(
                    ChannelMode::OrderedReliable(reliable(1.5, Some(1024))),
                    ChannelDirection::Bidirectional,
                )
                .with_criticality(ChannelCriticality::High),
            )
            .protocol_id(),
            "criticality must be covered"
        );
        assert_ne!(
            with_channel_settings(
                ChannelSettings::new(
                    ChannelMode::OrderedReliable(reliable(1.5, Some(1024))),
                    ChannelDirection::Bidirectional,
                )
                .with_criticality(ChannelCriticality::Low),
            )
            .protocol_id(),
            with_channel_settings(
                ChannelSettings::new(
                    ChannelMode::OrderedReliable(reliable(1.5, Some(1024))),
                    ChannelDirection::Bidirectional,
                )
                .with_criticality(ChannelCriticality::High),
            )
            .protocol_id(),
        );

        // --- tick buffer capacity (its own mode payload) ---
        let tick = |capacity: usize| {
            with_channel_settings(ChannelSettings::new(
                ChannelMode::TickBuffered(TickBufferSettings {
                    message_capacity: capacity,
                }),
                ChannelDirection::ClientToServer,
            ))
        };
        assert_ne!(
            tick(64).protocol_id(),
            tick(128).protocol_id(),
            "tick buffer message_capacity must be covered"
        );
    }

    /// F4b -- **Deployment-only settings are excluded.**
    ///
    /// Two peers must be free to differ on where they bind, what RTC path they
    /// serve, and whether a debug build is simulating packet loss. Folding
    /// those in would make the fingerprint refuse compatible peers, which is
    /// a worse failure than the one it exists to prevent.
    #[test]
    fn deployment_only_settings_are_not_part_of_the_fingerprint() {
        let plain = locked(|p| {
            p.add_message::<Whisper>();
        });
        let conditioned = locked(|p| {
            p.add_message::<Whisper>();
            p.link_condition(LinkConditionerConfig::good_condition());
        });
        let rehomed = locked(|p| {
            p.add_message::<Whisper>();
            p.rtc_endpoint("/somewhere_else".to_string());
        });

        assert_eq!(plain.protocol_id(), conditioned.protocol_id());
        assert_eq!(plain.protocol_id(), rehomed.protocol_id());
    }

    /// F4c -- **Compression is per-direction and carries its parameters.**
    ///
    /// A peer that decompresses with a different dictionary does not get a
    /// clean refusal, it gets garbage, so every part of the setting has to be
    /// in the fingerprint: whether each direction is compressed at all, which
    /// direction it is, the mode, and the mode's payload including the
    /// dictionary bytes themselves.
    #[test]
    fn a_change_to_any_compression_setting_changes_the_id() {
        let with = |config: Option<CompressionConfig>| {
            locked(|p| {
                p.add_message::<Whisper>();
                if let Some(config) = config {
                    p.compression(config);
                }
            })
        };

        let none = with(None);
        let both_off = with(Some(CompressionConfig::new(None, None)));
        // "no compression config at all" and "a config that compresses
        // nothing" are the same thing on the wire, but they must still be
        // distinguishable in the preimage only if they differ in behaviour --
        // they do not, so the presence byte is what separates them and this
        // asserts the current, deliberate encoding.
        assert_ne!(
            none.protocol_id(),
            both_off.protocol_id(),
            "the presence of a compression config is itself encoded"
        );

        let s2c = with(Some(CompressionConfig::new(
            Some(CompressionMode::Default(3)),
            None,
        )));
        let c2s = with(Some(CompressionConfig::new(
            None,
            Some(CompressionMode::Default(3)),
        )));
        assert_ne!(
            s2c.protocol_id(),
            c2s.protocol_id(),
            "compressing one direction must differ from compressing the other"
        );
        assert_ne!(s2c.protocol_id(), both_off.protocol_id());

        // Level is a payload, not just a variant.
        assert_ne!(
            s2c.protocol_id(),
            with(Some(CompressionConfig::new(
                Some(CompressionMode::Default(9)),
                None,
            )))
            .protocol_id(),
            "compression level must be covered"
        );

        // Mode variant, holding the level constant.
        assert_ne!(
            s2c.protocol_id(),
            with(Some(CompressionConfig::new(
                Some(CompressionMode::Dictionary(3, b"dictionary".to_vec())),
                None,
            )))
            .protocol_id(),
            "the compression mode variant must be covered"
        );

        // Dictionary identity: same mode, same level, different bytes.
        assert_ne!(
            with(Some(CompressionConfig::new(
                Some(CompressionMode::Dictionary(3, b"dictionary".to_vec())),
                None,
            )))
            .protocol_id(),
            with(Some(CompressionConfig::new(
                Some(CompressionMode::Dictionary(3, b"dictionaru".to_vec())),
                None,
            )))
            .protocol_id(),
            "the dictionary contents must be covered -- a peer with a different \
             dictionary decompresses to garbage"
        );

        // Training sample count.
        assert_ne!(
            with(Some(CompressionConfig::new(
                Some(CompressionMode::Training(100)),
                None,
            )))
            .protocol_id(),
            with(Some(CompressionConfig::new(
                Some(CompressionMode::Training(200)),
                None,
            )))
            .protocol_id(),
            "the training sample count must be covered"
        );
    }

    /// F4d -- **Client-authoritative entities is part of the protocol.**
    ///
    /// It decides whether the server accepts entity mutations the client
    /// originates, i.e. whether a whole class of message is legal on the wire.
    #[test]
    fn the_client_authoritative_entities_mode_changes_the_id() {
        let off = locked(|p| {
            p.add_component::<Ghost>();
        });
        let on = locked(|p| {
            p.add_component::<Ghost>();
            p.enable_client_authoritative_entities();
        });

        assert_ne!(off.protocol_id(), on.protocol_id());
    }

    /// F5 -- **The fingerprint is a pure function of the registrations.**
    ///
    /// Two peers are separate processes with separate allocators, separate
    /// `TypeId` layouts across builds, and separate hash-map iteration orders.
    /// Nothing about *this* process may leak into the value, or the two ends
    /// will disagree at runtime while every in-process test passes. The
    /// registries are `HashMap`-backed, so iteration order is the live hazard
    /// here; this builds the same protocol many times and requires one answer.
    #[test]
    fn the_id_does_not_depend_on_anything_local_to_this_process() {
        let build = || {
            locked(|p| {
                p.add_default_channels();
                p.add_channel_settings::<Gossip>(ChannelSettings::new(
                    ChannelMode::OrderedReliable(reliable(1.5, Some(1024))),
                    ChannelDirection::Bidirectional,
                ));
                p.add_message::<Whisper>();
                p.add_message::<Shout>();
                p.add_request::<Question>();
                p.add_component::<Ghost>();
                p.add_resource::<Wraith>();
                p.compression(CompressionConfig::new(
                    Some(CompressionMode::Dictionary(3, b"dictionary".to_vec())),
                    Some(CompressionMode::Training(50)),
                ));
            })
        };

        let first = build().protocol_id();
        for _ in 0..32 {
            assert_eq!(build().protocol_id(), first);
        }
    }

    /// The preimage begins with a domain-separation tag, so a digest computed
    /// under this grammar can never be confused with one computed under
    /// another. Pinned as a literal: changing it is a protocol-wide break and
    /// should have to be done on purpose, in a diff that says so.
    #[test]
    fn the_preimage_is_domain_separated_by_a_pinned_tag() {
        assert_eq!(super::PROTOCOL_FINGERPRINT_FORMAT, b"naia:pf:v1");
    }

    /// The codec grammar constant tracks naia's own framing, not the
    /// application's schema. It is one value for the whole protocol; there is
    /// no per-message version and no application-settable epoch.
    #[test]
    fn the_codec_grammar_constant_is_protocol_wide_and_not_application_settable() {
        assert_eq!(super::CODEC_GRAMMAR_VERSION, 1);

        // Nothing a builder can call changes it: two protocols with identical
        // registrations agree regardless of how they were configured.
        let a = locked(|p| {
            p.add_message::<Whisper>();
            p.tick_interval(Duration::from_millis(20));
        });
        let b = locked(|p| {
            p.add_message::<Whisper>();
            p.tick_interval(Duration::from_millis(80));
        });
        assert_eq!(a.protocol_id(), b.protocol_id());
    }

    /// `locked_protocol_id` exists because `Server::new` locks a protocol and
    /// then hands clones of it to two constructors, either of which may need
    /// the fingerprint. An unconditional `lock()` there panics with "Protocol
    /// already locked!"; this must not, and must return the same value the
    /// first lock cached.
    #[test]
    fn asking_a_locked_protocol_for_its_id_again_does_not_panic() {
        let mut protocol = Protocol::builder();
        protocol.add_message::<Whisper>();

        let first = protocol.locked_protocol_id();
        let second = protocol.locked_protocol_id();
        let third = protocol.clone().locked_protocol_id();

        assert_eq!(first, second);
        assert_eq!(first, third);
        assert_eq!(first, protocol.protocol_id());
    }

    /// The fingerprint is exactly 128 bits, and its hex form is exactly the
    /// width the socket layer validates against. The two constants live in
    /// different crates -- `naia-shared` cannot be seen from
    /// `naia-socket-shared` -- so their agreement is asserted rather than
    /// assumed. (A compile-time assertion in `protocol_id.rs` covers the same
    /// ground; this one states it where a reader of the fingerprint code will
    /// look.)
    #[test]
    fn the_fingerprint_is_128_bits_and_matches_the_header_width() {
        use crate::{ProtocolId, PROTOCOL_ID_HEADER_VALUE_LEN};

        let id = locked(|p| {
            p.add_message::<Whisper>();
        })
        .protocol_id();

        assert_eq!(ProtocolId::BYTE_LEN, 16);
        assert_eq!(id.bytes().len(), 16);
        assert_eq!(id.to_hex().len(), PROTOCOL_ID_HEADER_VALUE_LEN);
        assert!(id.to_hex().chars().all(|c| c.is_ascii_hexdigit()));
        assert_eq!(ProtocolId::from_hex(&id.to_hex()), Some(id));

        // A real protocol's fingerprint is not the all-zero default -- that
        // would mean the preimage never reached the hasher.
        assert_ne!(id, ProtocolId::default());
    }
}
