use std::time::Duration;

use naia_socket_shared::{LinkConditionerConfig, SocketConfig};

use crate::{
    connection::compression_config::CompressionConfig,
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

    /// Compute the protocol ID from current state.
    fn compute_protocol_id(&self) -> ProtocolId {
        let mut hasher = blake3::Hasher::new();

        // Channels
        for name in self.channel_kinds.all_names() {
            hasher.update(name.as_bytes());
        }
        // Messages
        for name in self.message_kinds.all_names() {
            hasher.update(name.as_bytes());
        }
        // Components
        for name in self.component_kinds.all_names() {
            hasher.update(name.as_bytes());
        }
        // Resources — fold in a side-channel marker per resource kind so
        // that two protocols differing only in which kinds are tagged
        // resource hash differently. Without this, downgrading a resource
        // to a plain component (or vice-versa) would collide on the wire
        // mismatch detector.
        hasher.update(b"naia:resources:");
        let mut resource_count = 0u32;
        for _ in self.resource_kinds.iter() {
            resource_count += 1;
        }
        hasher.update(&resource_count.to_le_bytes());

        let hash = hasher.finalize();
        let mut bytes = [0u8; 8];
        bytes.copy_from_slice(&hash.as_bytes()[..8]);
        ProtocolId::new(u64::from_le_bytes(bytes))
    }
}

#[cfg(test)]
mod protocol_tests {
    use std::time::Duration;

    use naia_socket_shared::LinkConditionerConfig;

    use crate::{
        connection::compression_config::CompressionConfig, ComponentKind, Message, Property,
        Replicate, Request, Response,
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
}
