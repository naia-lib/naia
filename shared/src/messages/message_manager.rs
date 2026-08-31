use std::collections::HashMap;

use log::{error, warn};
use naia_serde::{BitReader, BitWrite, BitWriter, Serde, SerdeErr};
use naia_socket_shared::Instant;

use crate::world::local::local_world_manager::LocalWorldManager;
use crate::{
    constants::FRAGMENTATION_LIMIT_BITS,
    messages::{
        channels::{
            channel::ChannelMode,
            channel::ChannelSettings,
            channel::ReliableSettings,
            channel_kinds::{ChannelKind, ChannelKinds},
            receivers::{
                channel_receiver::MessageChannelReceiver,
                ordered_reliable_receiver::OrderedReliableReceiver,
                sequenced_reliable_receiver::SequencedReliableReceiver,
                sequenced_unreliable_receiver::SequencedUnreliableReceiver,
                unordered_reliable_receiver::UnorderedReliableReceiver,
                unordered_unreliable_receiver::UnorderedUnreliableReceiver,
            },
            senders::{
                channel_sender::MessageChannelSender, message_fragmenter::MessageFragmenter,
                reliable_message_sender::ReliableMessageSender, request_sender::LocalResponseId,
                sequenced_unreliable_sender::SequencedUnreliableSender,
                unordered_unreliable_sender::UnorderedUnreliableSender,
            },
        },
        message_container::MessageContainer,
        request::GlobalRequestId,
    },
    types::{HostType, MessageIndex, PacketIndex},
    world::{
        entity::entity_converters::LocalEntityAndGlobalEntityConverterMut,
        remote::remote_entity_waitlist::RemoteEntityWaitlist,
    },
    LocalEntityAndGlobalEntityConverter, MessageKinds, PacketNotifiable,
};

type RequestsAndResponsesOut = (
    Vec<(ChannelKind, Vec<(LocalResponseId, MessageContainer)>)>,
    Vec<(GlobalRequestId, MessageContainer)>,
);

/// The receive window a reliable channel's receiver should enforce.
///
/// It is the channel's own `max_queue_depth`: that is exactly the span of message
/// indices a conforming peer's sender can have outstanding, so it is the tightest
/// window that never rejects honest traffic. Channels are declared once in the
/// shared `Protocol`, so both ends agree on the value by construction.
/// `max_queue_depth: None` opts out of the bound on both sides.
fn receive_window(settings: &ReliableSettings) -> Option<u16> {
    settings
        .max_queue_depth
        .map(|depth| u16::try_from(depth).unwrap_or(u16::MAX))
}

/// Handles incoming/outgoing messages, tracks the delivery status of Messages
/// so that guaranteed Messages can be re-transmitted to the remote host
pub struct MessageManager {
    channel_senders: HashMap<ChannelKind, Box<dyn MessageChannelSender>>,
    channel_receivers: HashMap<ChannelKind, Box<dyn MessageChannelReceiver>>,
    channel_settings: HashMap<ChannelKind, ChannelSettings>,
    #[cfg(feature = "observability")]
    channel_names: HashMap<ChannelKind, String>,
    packet_to_message_map: HashMap<PacketIndex, Vec<(ChannelKind, Vec<MessageIndex>)>>,
    message_fragmenter: MessageFragmenter,
}

impl MessageManager {
    /// Creates a new MessageManager
    pub fn new(host_type: HostType, channel_kinds: &ChannelKinds) -> Self {
        // initialize all reliable channels

        // initialize senders
        let mut channel_senders = HashMap::<ChannelKind, Box<dyn MessageChannelSender>>::new();
        for (channel_kind, channel_settings) in channel_kinds.channels() {
            //info!("initialize senders for channel: {:?}", channel_kind);
            match &host_type {
                HostType::Server => {
                    if !channel_settings.can_send_to_client() {
                        continue;
                    }
                }
                HostType::Client => {
                    if !channel_settings.can_send_to_server() {
                        continue;
                    }
                }
            }

            match &channel_settings.mode {
                ChannelMode::UnorderedUnreliable => {
                    channel_senders
                        .insert(channel_kind, Box::new(UnorderedUnreliableSender::new()));
                }
                ChannelMode::SequencedUnreliable => {
                    channel_senders
                        .insert(channel_kind, Box::new(SequencedUnreliableSender::new()));
                }
                ChannelMode::UnorderedReliable(settings)
                | ChannelMode::SequencedReliable(settings)
                | ChannelMode::OrderedReliable(settings) => {
                    channel_senders.insert(
                        channel_kind,
                        Box::new(ReliableMessageSender::new(
                            settings.rtt_resend_factor,
                            settings.max_queue_depth,
                        )),
                    );
                }
                ChannelMode::TickBuffered(_) => {
                    // Tick buffered channel uses another manager, skip
                }
            };
        }

        // initialize receivers
        let mut channel_receivers = HashMap::<ChannelKind, Box<dyn MessageChannelReceiver>>::new();
        for (channel_kind, channel_settings) in channel_kinds.channels() {
            match &host_type {
                HostType::Server => {
                    if !channel_settings.can_send_to_server() {
                        continue;
                    }
                }
                HostType::Client => {
                    if !channel_settings.can_send_to_client() {
                        continue;
                    }
                }
            }

            match &channel_settings.mode {
                ChannelMode::UnorderedUnreliable => {
                    channel_receivers
                        .insert(channel_kind, Box::new(UnorderedUnreliableReceiver::new()));
                }
                ChannelMode::SequencedUnreliable => {
                    channel_receivers
                        .insert(channel_kind, Box::new(SequencedUnreliableReceiver::new()));
                }
                ChannelMode::UnorderedReliable(settings) => {
                    channel_receivers.insert(
                        channel_kind,
                        Box::new(UnorderedReliableReceiver::with_window(receive_window(
                            settings,
                        ))),
                    );
                }
                ChannelMode::SequencedReliable(settings) => {
                    channel_receivers.insert(
                        channel_kind,
                        Box::new(SequencedReliableReceiver::with_window(receive_window(
                            settings,
                        ))),
                    );
                }
                ChannelMode::OrderedReliable(settings) => {
                    channel_receivers.insert(
                        channel_kind,
                        Box::new(OrderedReliableReceiver::with_window(receive_window(
                            settings,
                        ))),
                    );
                }
                ChannelMode::TickBuffered(_) => {
                    // Tick buffered channel uses another manager, skip
                }
            };
        }

        // initialize settings
        let mut channel_settings_map = HashMap::new();
        for (channel_kind, channel_settings) in channel_kinds.channels() {
            channel_settings_map.insert(channel_kind, channel_settings);
        }

        #[cfg(feature = "observability")]
        let channel_names = {
            let mut map = HashMap::new();
            for (kind, name) in channel_kinds.channel_names() {
                map.insert(kind, name);
            }
            map
        };

        Self {
            channel_senders,
            channel_receivers,
            channel_settings: channel_settings_map,
            #[cfg(feature = "observability")]
            channel_names,
            packet_to_message_map: HashMap::new(),
            message_fragmenter: MessageFragmenter::new(),
        }
    }

    // Outgoing Messages

    /// Queues a Message to be transmitted to the remote host. Returns `true`
    /// if the message was accepted, `false` if the channel queue was full and
    /// the message was dropped (reliable channels only — unreliable channels
    /// always return `true`, evicting the oldest queued message if needed).
    pub fn send_message(
        &mut self,
        message_kinds: &MessageKinds,
        converter: &mut dyn LocalEntityAndGlobalEntityConverterMut,
        channel_kind: &ChannelKind,
        message: MessageContainer,
    ) -> bool {
        #[cfg(feature = "observability")]
        if let Some(name) = self.channel_names.get(channel_kind) {
            metrics::counter!(crate::MESSAGES_SENT_TOTAL, "channel" => name.clone()).increment(1);
        }

        let Some(channel) = self.channel_senders.get_mut(channel_kind) else {
            panic!("Channel not configured correctly! Cannot send message.");
        };

        let message_bit_length = message.bit_length(message_kinds, converter);
        if message_bit_length > FRAGMENTATION_LIMIT_BITS {
            let Some(settings) = self.channel_settings.get(channel_kind) else {
                panic!("Channel not configured correctly! Cannot send message.");
            };
            if !settings.reliable() {
                error!("ERROR: Attempting to send Message above the fragmentation size limit over an unreliable Message channel! Slim down the size of your Message, or send this Message through a reliable message channel.");
                return false;
            }

            // Fragment the message and attempt to queue all fragments. If any
            // fragment is rejected (queue full), the partial send is logged and
            // the whole message is considered dropped.
            let messages =
                self.message_fragmenter
                    .fragment_message(message_kinds, converter, message);
            let mut all_accepted = true;
            for message_fragment in messages {
                if !channel.send_message(message_fragment) {
                    all_accepted = false;
                }
            }
            all_accepted
        } else {
            channel.send_message(message)
        }
    }

    /// Queues a request with `global_request_id` into the given channel's send buffer.
    ///
    /// Returns `false` if the channel refused it (reliable queue-depth cap
    /// reached); nothing was enqueued and the caller must retry later.
    pub fn send_request(
        &mut self,
        message_kinds: &MessageKinds,
        converter: &mut dyn LocalEntityAndGlobalEntityConverterMut,
        channel_kind: &ChannelKind,
        global_request_id: GlobalRequestId,
        request: MessageContainer,
    ) -> bool {
        let Some(channel) = self.channel_senders.get_mut(channel_kind) else {
            panic!("Channel not configured correctly! Cannot send message.");
        };
        channel.send_outgoing_request(message_kinds, converter, global_request_id, request)
    }

    /// Queues a response keyed by `local_response_id` into the given channel's send buffer.
    ///
    /// Returns `false` if the channel refused it (reliable queue-depth cap
    /// reached); nothing was enqueued and the caller must retry later.
    pub fn send_response(
        &mut self,
        message_kinds: &MessageKinds,
        converter: &mut dyn LocalEntityAndGlobalEntityConverterMut,
        channel_kind: &ChannelKind,
        local_response_id: LocalResponseId,
        response: MessageContainer,
    ) -> bool {
        let Some(channel) = self.channel_senders.get_mut(channel_kind) else {
            panic!("Channel not configured correctly! Cannot send message.");
        };
        channel.send_outgoing_response(message_kinds, converter, local_response_id, response)
    }

    /// Advances all channel senders, re-queuing any messages due for retransmission given current RTT.
    pub fn collect_outgoing_messages(&mut self, now: &Instant, rtt_millis: &f32) {
        for channel in self.channel_senders.values_mut() {
            channel.collect_messages(now, rtt_millis);
        }
    }

    /// Returns whether the Manager has queued Messages that can be transmitted
    /// to the remote host
    pub fn has_outgoing_messages(&self) -> bool {
        for channel in self.channel_senders.values() {
            if channel.has_messages() {
                return true;
            }
        }
        false
    }

    /// Encodes all pending outgoing messages across all channels into `writer`, ordered by channel criticality.
    pub fn write_messages(
        &mut self,
        channel_kinds: &ChannelKinds,
        message_kinds: &MessageKinds,
        converter: &mut dyn LocalEntityAndGlobalEntityConverterMut,
        writer: &mut BitWriter,
        packet_index: PacketIndex,
        has_written: &mut bool,
    ) {
        // Phase A: walk channels in descending criticality order so High
        // (e.g. TickBuffered) wins packet space over Normal wins over Low
        // under tight budgets. Stable sort preserves equal-gain order.
        // Reverse bits of base_gain into an ordering key so higher base_gain
        // sorts first; ties broken by ChannelKind order (stable).
        let mut ordered: Vec<(ChannelKind, f32)> = self
            .channel_senders
            .keys()
            .map(|k| {
                let gain = self
                    .channel_settings
                    .get(k)
                    .map(|s| s.criticality.base_gain())
                    .unwrap_or(1.0);
                (*k, gain)
            })
            .collect();
        ordered.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        for (channel_kind, _gain) in &ordered {
            let channel = self.channel_senders.get_mut(channel_kind).unwrap();
            if !channel.has_messages() {
                continue;
            }

            // check that we can at least write a ChannelIndex and a MessageContinue bit
            let mut counter = writer.counter();
            // reserve MessageContinue bit
            counter.write_bit(false);
            // write ChannelContinue bit
            counter.write_bit(false);
            // write ChannelIndex (variable-width — count the actual bits this
            // channel will take rather than a const upper bound)
            channel_kind.ser(channel_kinds, &mut counter);
            if counter.overflowed() {
                break;
            }

            // reserve MessageContinue bit
            writer.reserve_bits(1);
            // write ChannelContinue bit
            true.ser(writer);
            // write ChannelIndex
            channel_kind.ser(channel_kinds, writer);
            // write Messages
            if let Some(message_indices) =
                channel.write_messages(message_kinds, converter, writer, has_written)
            {
                self.packet_to_message_map.entry(packet_index).or_default();
                let channel_list = self.packet_to_message_map.get_mut(&packet_index).unwrap();
                channel_list.push((*channel_kind, message_indices));
            }

            // write MessageContinue finish bit, release
            writer.release_bits(1);
            false.ser(writer);
        }

        // write ChannelContinue finish bit, release
        writer.release_bits(1);
        false.ser(writer);
    }

    // Incoming Messages

    /// Parses an incoming message packet, routing each message to its channel's receiver buffer.
    pub fn read_messages(
        &mut self,
        channel_kinds: &ChannelKinds,
        message_kinds: &MessageKinds,
        local_world_manager: &mut LocalWorldManager,
        reader: &mut BitReader,
    ) -> Result<(), SerdeErr> {
        loop {
            let message_continue = bool::de(reader)?;
            if !message_continue {
                break;
            }

            // read channel id
            let channel_kind = ChannelKind::de(channel_kinds, reader)?;

            // continue read inside channel
            let Some(channel) = self.channel_receivers.get_mut(&channel_kind) else {
                // Corrupt packet: channel kind decoded to a value not registered in this
                // connection's channel set. Treat as a deserialization failure.
                return Err(SerdeErr);
            };
            channel.read_messages(message_kinds, local_world_manager, reader)?;
        }

        Ok(())
    }

    /// Retrieve all messages from the channel buffers
    pub fn receive_messages(
        &mut self,
        message_kinds: &MessageKinds,
        now: &Instant,
        entity_converter: &dyn LocalEntityAndGlobalEntityConverter,
        entity_waitlist: &mut RemoteEntityWaitlist,
    ) -> Vec<(ChannelKind, Vec<MessageContainer>)> {
        let mut output = Vec::new();
        // TODO: shouldn't we have a priority mechanisms between channels?
        for (channel_kind, channel) in &mut self.channel_receivers {
            let messages =
                channel.receive_messages(message_kinds, now, entity_waitlist, entity_converter);
            output.push((*channel_kind, messages));
        }
        output
    }

    /// Retrieve all requests from the channel buffers
    pub fn receive_requests_and_responses(&mut self) -> RequestsAndResponsesOut {
        let mut request_output = Vec::new();
        let mut response_output = Vec::new();
        for (channel_kind, channel) in &mut self.channel_receivers {
            if !self
                .channel_settings
                .get(channel_kind)
                .unwrap()
                .can_request_and_respond()
            {
                continue;
            }

            let (requests, responses) = channel.receive_requests_and_responses();
            if !requests.is_empty() {
                request_output.push((*channel_kind, requests));
            }

            if !responses.is_empty() {
                let Some(channel_sender) = self.channel_senders.get_mut(channel_kind) else {
                    panic!(
                        "Channel not configured correctly! Cannot send message on channel: {:?}",
                        channel_kind
                    );
                };
                for (local_request_id, response) in responses {
                    // The id this response claims to answer is read off the
                    // wire. A peer can answer a request that was never made, or
                    // answer the same one twice, so an id with no outstanding
                    // request is malformed input rather than a local invariant.
                    // `LocalRequestId` is a single byte, so the whole space is
                    // trivially reachable by a hostile peer.
                    let Some(global_request_id) =
                        channel_sender.process_incoming_response(&local_request_id)
                    else {
                        warn!(
                            "dropping a response on channel {:?} that answers no outstanding request",
                            channel_kind
                        );
                        continue;
                    };
                    response_output.push((global_request_id, response));
                }
            }
        }
        (request_output, response_output)
    }
}

impl PacketNotifiable for MessageManager {
    /// Occurs when a packet has been notified as delivered. Stops tracking the
    /// status of Messages in that packet.
    fn notify_packet_delivered(&mut self, packet_index: PacketIndex) {
        if let Some(channel_list) = self.packet_to_message_map.get(&packet_index) {
            for (channel_kind, message_indices) in channel_list {
                if let Some(channel) = self.channel_senders.get_mut(channel_kind) {
                    for message_index in message_indices {
                        channel.notify_message_delivered(message_index);
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod unsolicited_response_tests {
    use naia_serde::{BitReader, SerdeErr};
    use naia_socket_shared::Instant;

    use crate::{
        messages::fragment::{FragmentId, FragmentIndex, FragmentedMessage},
        messages::{
            channels::receivers::channel_receiver::{
                ChannelReceiver, MessageChannelReceiver, RequestsAndResponses,
            },
            message_container::MessageContainer,
        },
        world::{
            local::local_world_manager::LocalWorldManager,
            remote::remote_entity_waitlist::RemoteEntityWaitlist,
        },
        Channel, ChannelDirection, ChannelKind, ChannelKinds, ChannelMode, ChannelSettings,
        HostType, LocalEntityAndGlobalEntityConverter, LocalResponseId, MessageKinds, Named,
        ReliableSettings,
    };

    use super::MessageManager;

    struct RequestChannel;
    impl Named for RequestChannel {
        fn name(&self) -> String {
            "RequestChannel".to_string()
        }
        fn protocol_name() -> &'static str {
            "RequestChannel"
        }
    }
    impl Channel for RequestChannel {}

    /// A receiver that hands the manager one response whose id answers no
    /// outstanding request, standing in for a peer that made one up. The id is
    /// a single byte on the wire, so every value is reachable.
    struct UnsolicitedResponseReceiver {
        response: Option<(crate::LocalRequestId, MessageContainer)>,
    }

    impl ChannelReceiver<MessageContainer> for UnsolicitedResponseReceiver {
        fn receive_messages(
            &mut self,
            _message_kinds: &MessageKinds,
            _now: &Instant,
            _entity_waitlist: &mut RemoteEntityWaitlist,
            _converter: &dyn LocalEntityAndGlobalEntityConverter,
        ) -> Vec<MessageContainer> {
            Vec::new()
        }
    }

    impl MessageChannelReceiver for UnsolicitedResponseReceiver {
        fn read_messages(
            &mut self,
            _message_kinds: &MessageKinds,
            _local_world_manager: &mut LocalWorldManager,
            _reader: &mut BitReader,
        ) -> Result<(), SerdeErr> {
            Ok(())
        }

        fn receive_requests_and_responses(&mut self) -> RequestsAndResponses {
            (Vec::new(), self.response.take().into_iter().collect())
        }
    }

    /// Any message body will do -- the id is what is under test.
    fn filler_message() -> MessageContainer {
        MessageContainer::new(Box::new(FragmentedMessage::new(
            FragmentId::zero(),
            FragmentIndex::from_u32(0),
            Box::new([0u8; 4]),
        )))
    }

    /// Before this was guarded, a response naming a request id the host never
    /// issued unwrapped a `None` and took the process down -- from one packet,
    /// on whichever side received it.
    #[test]
    fn a_response_to_a_request_that_was_never_made_is_dropped() {
        let mut channel_kinds = ChannelKinds::new();
        channel_kinds.add_channel::<RequestChannel>(ChannelSettings::new(
            ChannelMode::UnorderedReliable(ReliableSettings::default()),
            ChannelDirection::Bidirectional,
        ));

        let mut manager = MessageManager::new(HostType::Client, &channel_kinds);
        let channel_kind = ChannelKind::of::<RequestChannel>();
        assert!(
            manager.channel_receivers.contains_key(&channel_kind),
            "the channel under test must be request-capable",
        );

        let bogus_id = LocalResponseId::from_raw(0x2a).receive_from_remote();
        manager.channel_receivers.insert(
            channel_kind,
            Box::new(UnsolicitedResponseReceiver {
                response: Some((bogus_id, filler_message())),
            }),
        );

        let (requests, responses) = manager.receive_requests_and_responses();

        assert!(requests.is_empty());
        assert!(
            responses.is_empty(),
            "an unsolicited response must be dropped, not surfaced",
        );
    }
}

#[cfg(test)]
mod message_manager_tests {
    //! Channel wiring, packing order and packet framing for [`MessageManager`].
    //!
    //! The manager owns three things nothing else does: which channels get a
    //! sender and which get a receiver (a direction-dependent asymmetry that is
    //! easy to invert), the order channels are offered packet space in, and the
    //! nested continue-bit framing that wraps every channel's own message
    //! stream. Those are what the tests below pin.

    use std::{any::Any, collections::HashSet, net::SocketAddr};

    use naia_serde::{BitCounter, BitReader, BitWrite, BitWriter, Serde, SerdeErr};
    use naia_socket_shared::Instant;

    use crate::messages::{
        channels::senders::request_sender::LocalRequestId, message::Message,
        message_kinds::MessageKind,
    };
    use crate::world::entity::entity_converters::{
        LocalEntityAndGlobalEntityConverter, LocalEntityAndGlobalEntityConverterMut,
    };
    use crate::world::remote::remote_entity_waitlist::RemoteEntityWaitlist;
    use crate::{
        constants::FRAGMENTATION_LIMIT_BITS,
        messages::fragment::{FragmentId, FragmentIndex, FragmentedMessage},
        world::{local::local_world_manager::LocalWorldManager, test_support::TestGwm},
        Channel, ChannelCriticality, ChannelDirection, ChannelKind, ChannelKinds, ChannelMode,
        ChannelSettings, ComponentKinds, FakeEntityConverter, GlobalRequestId, HostType,
        MessageBuilder, MessageContainer, MessageKinds, Named, PacketNotifiable, ReliableSettings,
        RequestOrResponse,
    };

    use super::{receive_window, MessageManager};

    // -- channel zoo --------------------------------------------------------

    macro_rules! test_channel {
        ($name:ident) => {
            struct $name;
            impl Named for $name {
                fn name(&self) -> String {
                    stringify!($name).to_string()
                }
                fn protocol_name() -> &'static str {
                    stringify!($name)
                }
            }
            impl Channel for $name {}
        };
    }

    test_channel!(ToClient);
    test_channel!(ToServer);
    test_channel!(BothWays);
    test_channel!(LowPriority);
    test_channel!(HighPriority);
    test_channel!(Unreliable);

    fn reliable(direction: ChannelDirection) -> ChannelSettings {
        ChannelSettings::new(
            ChannelMode::OrderedReliable(ReliableSettings::default()),
            direction,
        )
    }

    fn unreliable(direction: ChannelDirection) -> ChannelSettings {
        ChannelSettings::new(ChannelMode::UnorderedUnreliable, direction)
    }

    /// One channel of each direction, so the sender/receiver split in `new()`
    /// is observable from either host type.
    fn directional_kinds() -> ChannelKinds {
        let mut kinds = ChannelKinds::new();
        kinds.add_channel::<ToClient>(reliable(ChannelDirection::ServerToClient));
        kinds.add_channel::<ToServer>(reliable(ChannelDirection::ClientToServer));
        kinds.add_channel::<BothWays>(reliable(ChannelDirection::Bidirectional));
        kinds
    }

    /// A minimal, fully round-trippable message. `FragmentedMessage` is
    /// unsuitable for the round-trip test because receivers treat fragments
    /// specially and try to reassemble them; this one is an ordinary message
    /// carrying a single tag byte.
    #[derive(Clone)]
    struct Ping(u8);

    impl Named for Ping {
        fn name(&self) -> String {
            "Ping".into()
        }
        fn protocol_name() -> &'static str {
            "Ping"
        }
    }

    struct PingBuilder;

    impl MessageBuilder for PingBuilder {
        fn read(
            &self,
            reader: &mut BitReader,
            _: &dyn LocalEntityAndGlobalEntityConverter,
        ) -> Result<MessageContainer, SerdeErr> {
            Ok(MessageContainer::new(Box::new(Ping(u8::de(reader)?))))
        }
        fn box_clone(&self) -> Box<dyn MessageBuilder> {
            Box::new(PingBuilder)
        }
    }

    impl Message for Ping {
        fn kind(&self) -> MessageKind {
            MessageKind::of::<Self>()
        }
        fn to_boxed_any(self: Box<Self>) -> Box<dyn Any> {
            self
        }
        fn create_builder() -> Box<dyn MessageBuilder> {
            Box::new(PingBuilder)
        }
        fn bit_length(
            &self,
            _: &MessageKinds,
            _: &mut dyn LocalEntityAndGlobalEntityConverterMut,
        ) -> u32 {
            8
        }
        fn is_fragment(&self) -> bool {
            false
        }
        fn is_request(&self) -> bool {
            false
        }
        fn write(
            &self,
            message_kinds: &MessageKinds,
            writer: &mut dyn BitWrite,
            _: &mut dyn LocalEntityAndGlobalEntityConverterMut,
        ) {
            self.kind().ser(message_kinds, writer);
            self.0.ser(writer);
        }
        fn relations_waiting(&self) -> Option<HashSet<crate::RemoteEntity>> {
            None
        }
        fn relations_complete(&mut self, _: &dyn LocalEntityAndGlobalEntityConverter) {}
    }

    /// A message whose encoded length is exactly the number of bits it is built
    /// with. `FragmentedMessage` is byte-granular, so the largest payload that
    /// fits lands *under* the fragmentation limit rather than on it -- which
    /// leaves the `>` / `>=` boundary itself untested. This one lands on it.
    #[derive(Clone)]
    struct Exact(u32);

    impl Named for Exact {
        fn name(&self) -> String {
            "Exact".into()
        }
        fn protocol_name() -> &'static str {
            "Exact"
        }
    }

    struct ExactBuilder;

    impl MessageBuilder for ExactBuilder {
        fn read(
            &self,
            _: &mut BitReader,
            _: &dyn LocalEntityAndGlobalEntityConverter,
        ) -> Result<MessageContainer, SerdeErr> {
            // Padding bits carry no self-describing length. `Exact` exists to be
            // measured and queued, never to come back off the wire.
            unreachable!("Exact is never read back")
        }
        fn box_clone(&self) -> Box<dyn MessageBuilder> {
            Box::new(ExactBuilder)
        }
    }

    impl Message for Exact {
        fn kind(&self) -> MessageKind {
            MessageKind::of::<Self>()
        }
        fn to_boxed_any(self: Box<Self>) -> Box<dyn Any> {
            self
        }
        fn create_builder() -> Box<dyn MessageBuilder> {
            Box::new(ExactBuilder)
        }
        fn bit_length(
            &self,
            _: &MessageKinds,
            _: &mut dyn LocalEntityAndGlobalEntityConverterMut,
        ) -> u32 {
            self.0
        }
        fn is_fragment(&self) -> bool {
            false
        }
        fn is_request(&self) -> bool {
            false
        }
        fn write(
            &self,
            message_kinds: &MessageKinds,
            writer: &mut dyn BitWrite,
            _: &mut dyn LocalEntityAndGlobalEntityConverterMut,
        ) {
            let mut counter = BitCounter::new(0, 0, u32::MAX);
            self.kind().ser(message_kinds, &mut counter);
            let kind_bits = counter.bits_needed();
            self.kind().ser(message_kinds, writer);
            for _ in kind_bits..self.0 {
                writer.write_bit(false);
            }
        }
        fn relations_waiting(&self) -> Option<HashSet<crate::RemoteEntity>> {
            None
        }
        fn relations_complete(&mut self, _: &dyn LocalEntityAndGlobalEntityConverter) {}
    }

    fn exact(bits: u32) -> MessageContainer {
        MessageContainer::new(Box::new(Exact(bits)))
    }

    fn ping(tag: u8) -> MessageContainer {
        MessageContainer::new(Box::new(Ping(tag)))
    }

    fn message_kinds() -> MessageKinds {
        let mut kinds = MessageKinds::new();
        kinds.add_message::<FragmentedMessage>();
        kinds.add_message::<Ping>();
        kinds.add_message::<Exact>();
        kinds.add_message::<RequestOrResponse>();
        kinds
    }

    fn tagged(tag: u8, len: usize) -> MessageContainer {
        let mut bytes = vec![0u8; len];
        bytes[0] = tag;
        MessageContainer::new(Box::new(FragmentedMessage::new(
            FragmentId::zero(),
            FragmentIndex::from_u32(0),
            bytes.into_boxed_slice(),
        )))
    }

    /// Moves everything one manager has queued into the other, the way a
    /// connection would: collect, write one packet, read it, then drain the
    /// receivers (which is what routes request/response envelopes into their
    /// own buffers).
    fn deliver(
        from: &mut MessageManager,
        to: &mut MessageManager,
        kinds: &ChannelKinds,
        messages: &MessageKinds,
    ) {
        from.collect_outgoing_messages(&Instant::now(), &200.0);
        let mut writer = BitWriter::new();
        let mut has_written = false;
        from.write_messages(
            kinds,
            messages,
            &mut FakeEntityConverter,
            &mut writer,
            0,
            &mut has_written,
        );
        assert!(has_written, "the sender should have written a packet");
        let bytes = writer.to_bytes();

        let (_gwm, mut world) = world_manager();
        to.read_messages(kinds, messages, &mut world, &mut BitReader::new(&bytes))
            .expect("a packet the counterpart just wrote should parse");
        let mut waitlist = RemoteEntityWaitlist::new();
        to.receive_messages(
            messages,
            &Instant::now(),
            &FakeEntityConverter,
            &mut waitlist,
        );
    }

    fn world_manager() -> (TestGwm, LocalWorldManager) {
        let component_kinds = ComponentKinds::new();
        let gwm = TestGwm::new(&component_kinds);
        let address: Option<SocketAddr> = Some("127.0.0.1:4000".parse().unwrap());
        let manager = LocalWorldManager::new(&address, HostType::Client, 1, &gwm);
        (gwm, manager)
    }

    // -- receive_window -----------------------------------------------------

    #[test]
    fn the_receive_window_is_the_channels_own_queue_depth() {
        assert_eq!(
            receive_window(&ReliableSettings {
                rtt_resend_factor: 1.5,
                max_queue_depth: Some(64),
            }),
            Some(64),
            "the window should be exactly the send cap, which is the tightest \
             bound that never rejects honest traffic"
        );
    }

    #[test]
    fn an_unbounded_queue_opts_out_of_the_receive_window() {
        assert_eq!(
            receive_window(&ReliableSettings {
                rtt_resend_factor: 1.5,
                max_queue_depth: None,
            }),
            None,
            "None disables the send cap and the receive window together"
        );
    }

    #[test]
    fn a_queue_depth_beyond_u16_saturates_rather_than_wrapping() {
        // MessageIndex is a u16, so a depth of 100_000 cannot be represented as
        // a window. Truncating instead of saturating would produce a window of
        // 34_464 -- far TIGHTER than configured -- and silently drop honest
        // traffic from a conforming peer.
        assert_eq!(
            receive_window(&ReliableSettings {
                rtt_resend_factor: 1.5,
                max_queue_depth: Some(100_000),
            }),
            Some(u16::MAX)
        );
        assert_eq!(
            receive_window(&ReliableSettings {
                rtt_resend_factor: 1.5,
                max_queue_depth: Some(u16::MAX as usize),
            }),
            Some(u16::MAX),
            "the largest representable depth is not itself saturated away"
        );
    }

    // -- who gets a sender and who gets a receiver --------------------------

    #[test]
    fn a_server_sends_only_on_channels_that_reach_the_client() {
        let kinds = directional_kinds();
        let mut manager = MessageManager::new(HostType::Server, &kinds);
        let messages = message_kinds();

        assert!(manager.send_message(
            &messages,
            &mut FakeEntityConverter,
            &ChannelKind::of::<ToClient>(),
            tagged(1, 4)
        ));
        assert!(manager.send_message(
            &messages,
            &mut FakeEntityConverter,
            &ChannelKind::of::<BothWays>(),
            tagged(1, 4)
        ));
    }

    #[test]
    #[should_panic(expected = "Channel not configured correctly")]
    fn a_server_has_no_sender_for_a_client_to_server_channel() {
        // The direction filter in `new()` is the only thing standing between a
        // protocol author and a server that writes on a channel the client never
        // reads. Panicking names the mistake at the send site.
        let kinds = directional_kinds();
        let mut manager = MessageManager::new(HostType::Server, &kinds);
        manager.send_message(
            &message_kinds(),
            &mut FakeEntityConverter,
            &ChannelKind::of::<ToServer>(),
            tagged(1, 4),
        );
    }

    #[test]
    #[should_panic(expected = "Channel not configured correctly")]
    fn a_client_has_no_sender_for_a_server_to_client_channel() {
        let kinds = directional_kinds();
        let mut manager = MessageManager::new(HostType::Client, &kinds);
        manager.send_message(
            &message_kinds(),
            &mut FakeEntityConverter,
            &ChannelKind::of::<ToClient>(),
            tagged(1, 4),
        );
    }

    #[test]
    fn a_client_sends_only_on_channels_that_reach_the_server() {
        let kinds = directional_kinds();
        let mut manager = MessageManager::new(HostType::Client, &kinds);
        let messages = message_kinds();

        assert!(manager.send_message(
            &messages,
            &mut FakeEntityConverter,
            &ChannelKind::of::<ToServer>(),
            tagged(1, 4)
        ));
        assert!(manager.send_message(
            &messages,
            &mut FakeEntityConverter,
            &ChannelKind::of::<BothWays>(),
            tagged(1, 4)
        ));
    }

    #[test]
    fn a_receiver_only_exists_for_channels_the_peer_can_send_on() {
        // The receiver side is the mirror of the sender side: a server reads
        // client-to-server channels. Reading a packet that names a channel we
        // have no receiver for is a corrupt packet, not a panic.
        let kinds = directional_kinds();
        let mut server = MessageManager::new(HostType::Server, &kinds);
        let (_gwm, mut world) = world_manager();

        // Frame a packet naming the ServerToClient channel, which no server
        // receiver exists for.
        let mut writer = BitWriter::new();
        true.ser(&mut writer);
        ChannelKind::of::<ToClient>().ser(&kinds, &mut writer);
        let bytes = writer.to_bytes();

        let result = server.read_messages(
            &kinds,
            &message_kinds(),
            &mut world,
            &mut BitReader::new(&bytes),
        );
        assert!(
            result.is_err(),
            "a packet naming a channel this host does not receive on is malformed \
             input and must be rejected, not routed"
        );
    }

    // -- fragmentation gate -------------------------------------------------

    /// Bit length of a `FragmentedMessage` carrying `len` payload bytes, as the
    /// manager measures it.
    fn bit_length_of(len: usize) -> u32 {
        tagged(0, len).bit_length(&message_kinds(), &mut FakeEntityConverter)
    }

    /// The largest payload whose encoded message still fits under the
    /// fragmentation limit.
    fn payload_at_limit() -> usize {
        let mut len = 1;
        while bit_length_of(len + 1) <= FRAGMENTATION_LIMIT_BITS {
            len += 1;
        }
        len
    }

    #[test]
    fn a_message_at_exactly_the_fragmentation_limit_is_sent_whole() {
        // The gate is `>`, not `>=`: a body of exactly the limit fits in one
        // fragment, so fragmenting it would only add header overhead. This also
        // means an UNRELIABLE channel can still carry it.
        let len = payload_at_limit();
        assert!(bit_length_of(len) <= FRAGMENTATION_LIMIT_BITS);
        assert!(bit_length_of(len + 1) > FRAGMENTATION_LIMIT_BITS);

        let mut kinds = ChannelKinds::new();
        kinds.add_channel::<Unreliable>(unreliable(ChannelDirection::Bidirectional));
        let mut manager = MessageManager::new(HostType::Client, &kinds);

        assert!(
            manager.send_message(
                &message_kinds(),
                &mut FakeEntityConverter,
                &ChannelKind::of::<Unreliable>(),
                tagged(1, len)
            ),
            "a message that fits under the limit is accepted on an unreliable channel"
        );
    }

    #[test]
    fn an_oversized_message_is_refused_by_an_unreliable_channel() {
        // One byte past the limit. There is no fragment reassembly on an
        // unreliable channel, so the manager refuses rather than sending
        // fragments that can never be put back together.
        let len = payload_at_limit() + 1;

        let mut kinds = ChannelKinds::new();
        kinds.add_channel::<Unreliable>(unreliable(ChannelDirection::Bidirectional));
        let mut manager = MessageManager::new(HostType::Client, &kinds);

        assert!(
            !manager.send_message(
                &message_kinds(),
                &mut FakeEntityConverter,
                &ChannelKind::of::<Unreliable>(),
                tagged(1, len)
            ),
            "an oversized message on an unreliable channel must be rejected"
        );
        assert!(
            !manager.has_outgoing_messages(),
            "and nothing may be left queued behind it"
        );
    }

    #[test]
    fn an_oversized_message_is_fragmented_onto_a_reliable_channel() {
        let len = payload_at_limit() * 3;

        let mut kinds = ChannelKinds::new();
        kinds.add_channel::<BothWays>(reliable(ChannelDirection::Bidirectional));
        let mut manager = MessageManager::new(HostType::Client, &kinds);

        assert!(
            manager.send_message(
                &message_kinds(),
                &mut FakeEntityConverter,
                &ChannelKind::of::<BothWays>(),
                tagged(1, len)
            ),
            "a reliable channel accepts an oversized message by fragmenting it"
        );
        assert!(
            !manager.has_outgoing_messages(),
            "a reliable sender holds a newly queued message back until it is \
             collected: `send_message` enqueues, `collect_outgoing_messages` is \
             what makes it eligible for a packet"
        );
        manager.collect_outgoing_messages(&Instant::now(), &200.0);
        assert!(
            manager.has_outgoing_messages(),
            "once collected, the fragments are ready to write"
        );
    }

    // -- packing order ------------------------------------------------------

    /// Reads back the channel kinds, in order, from a packet the manager wrote.
    fn channels_in_packet(bytes: &[u8], kinds: &ChannelKinds) -> Vec<ChannelKind> {
        let mut reader = BitReader::new(bytes);
        let mut out = Vec::new();
        while bool::de(&mut reader).expect("a channel continue bit should be readable") {
            let channel_kind =
                ChannelKind::de(kinds, &mut reader).expect("the channel kind should read back");
            out.push(channel_kind);
            // Skip this channel's message stream.
            while bool::de(&mut reader).expect("a message continue bit should be readable") {
                message_kinds()
                    .read(&mut reader, &FakeEntityConverter)
                    .expect("a message the manager just wrote should read back");
            }
        }
        out
    }

    #[test]
    fn channels_are_offered_packet_space_in_descending_criticality() {
        // Under a tight packet budget the High channel must get its bytes first.
        // The map iteration order is arbitrary, so without the sort this test
        // would be flaky rather than merely wrong -- which is exactly why the
        // ordering deserves a test rather than a comment.
        let mut kinds = ChannelKinds::new();
        kinds.add_channel::<LowPriority>(
            unreliable(ChannelDirection::Bidirectional).with_criticality(ChannelCriticality::Low),
        );
        kinds.add_channel::<HighPriority>(
            unreliable(ChannelDirection::Bidirectional).with_criticality(ChannelCriticality::High),
        );

        let messages = message_kinds();
        let mut manager = MessageManager::new(HostType::Client, &kinds);
        manager.send_message(
            &messages,
            &mut FakeEntityConverter,
            &ChannelKind::of::<LowPriority>(),
            tagged(1, 4),
        );
        manager.send_message(
            &messages,
            &mut FakeEntityConverter,
            &ChannelKind::of::<HighPriority>(),
            tagged(2, 4),
        );

        let mut writer = BitWriter::new();
        let mut has_written = false;
        manager.write_messages(
            &kinds,
            &messages,
            &mut FakeEntityConverter,
            &mut writer,
            0,
            &mut has_written,
        );
        let bytes = writer.to_bytes();

        assert_eq!(
            channels_in_packet(&bytes, &kinds),
            vec![
                ChannelKind::of::<HighPriority>(),
                ChannelKind::of::<LowPriority>()
            ],
            "the High channel must be offered space before the Low one"
        );
    }

    #[test]
    fn a_channel_with_nothing_queued_is_skipped_entirely() {
        let mut kinds = ChannelKinds::new();
        kinds.add_channel::<LowPriority>(unreliable(ChannelDirection::Bidirectional));
        kinds.add_channel::<HighPriority>(unreliable(ChannelDirection::Bidirectional));

        let messages = message_kinds();
        let mut manager = MessageManager::new(HostType::Client, &kinds);
        manager.send_message(
            &messages,
            &mut FakeEntityConverter,
            &ChannelKind::of::<HighPriority>(),
            tagged(2, 4),
        );

        let mut writer = BitWriter::new();
        let mut has_written = false;
        manager.write_messages(
            &kinds,
            &messages,
            &mut FakeEntityConverter,
            &mut writer,
            0,
            &mut has_written,
        );
        let bytes = writer.to_bytes();

        assert_eq!(
            channels_in_packet(&bytes, &kinds),
            vec![ChannelKind::of::<HighPriority>()],
            "an empty channel should not even cost a channel header"
        );
    }

    #[test]
    fn writing_an_empty_manager_produces_only_the_terminating_bit() {
        let kinds = directional_kinds();
        let mut manager = MessageManager::new(HostType::Client, &kinds);

        let mut writer = BitWriter::new();
        let mut has_written = false;
        manager.write_messages(
            &kinds,
            &message_kinds(),
            &mut FakeEntityConverter,
            &mut writer,
            0,
            &mut has_written,
        );
        let bytes = writer.to_bytes();

        assert!(
            channels_in_packet(&bytes, &kinds).is_empty(),
            "nothing queued means no channels in the packet"
        );
        assert!(
            !has_written,
            "and `has_written` must stay false so the caller knows the packet is \
             still empty"
        );
    }

    // -- round trip ---------------------------------------------------------

    #[test]
    fn a_packet_written_by_one_manager_is_read_by_its_counterpart() {
        // The client writes, the server reads, and the messages come back out of
        // the channel they went in on. This is what the nested continue-bit
        // framing exists for.
        let kinds = directional_kinds();
        let messages = message_kinds();
        let mut client = MessageManager::new(HostType::Client, &kinds);
        let mut server = MessageManager::new(HostType::Server, &kinds);

        client.send_message(
            &messages,
            &mut FakeEntityConverter,
            &ChannelKind::of::<ToServer>(),
            ping(1),
        );
        client.send_message(
            &messages,
            &mut FakeEntityConverter,
            &ChannelKind::of::<ToServer>(),
            ping(2),
        );
        // A reliable sender only offers collected messages to a packet.
        client.collect_outgoing_messages(&Instant::now(), &200.0);

        let mut writer = BitWriter::new();
        let mut has_written = false;
        client.write_messages(
            &kinds,
            &messages,
            &mut FakeEntityConverter,
            &mut writer,
            0,
            &mut has_written,
        );
        assert!(has_written, "the client should have written something");
        let bytes = writer.to_bytes();

        let (_gwm, mut world) = world_manager();
        server
            .read_messages(&kinds, &messages, &mut world, &mut BitReader::new(&bytes))
            .expect("a packet the counterpart just wrote should parse");

        let mut waitlist = RemoteEntityWaitlist::new();
        let received = server.receive_messages(
            &messages,
            &Instant::now(),
            &FakeEntityConverter,
            &mut waitlist,
        );
        let on_channel: Vec<_> = received
            .into_iter()
            .filter(|(channel_kind, _)| *channel_kind == ChannelKind::of::<ToServer>())
            .flat_map(|(_, messages)| messages)
            .collect();
        let tags: Vec<u8> = on_channel
            .into_iter()
            .map(|message| {
                message
                    .to_boxed_any()
                    .downcast::<Ping>()
                    .expect("the round trip should yield the message that went in")
                    .0
            })
            .collect();
        assert_eq!(
            tags,
            vec![1, 2],
            "both messages should arrive on the channel they were sent on, in order"
        );
    }

    #[test]
    fn an_empty_packet_reads_as_success() {
        let kinds = directional_kinds();
        let mut manager = MessageManager::new(HostType::Server, &kinds);
        let (_gwm, mut world) = world_manager();

        let mut writer = BitWriter::new();
        false.ser(&mut writer);
        let bytes = writer.to_bytes();

        assert!(
            manager
                .read_messages(
                    &kinds,
                    &message_kinds(),
                    &mut world,
                    &mut BitReader::new(&bytes)
                )
                .is_ok(),
            "a packet with no channels is well-formed"
        );
    }

    // -- delivery notification ----------------------------------------------

    #[test]
    fn notifying_an_unknown_packet_is_harmless() {
        // Acks can arrive for packets that carried no reliable messages, so an
        // index with no recorded channel list is normal traffic, not an error.
        let kinds = directional_kinds();
        let mut manager = MessageManager::new(HostType::Client, &kinds);
        manager.notify_packet_delivered(99);
    }

    #[test]
    fn a_reliable_message_stops_being_resent_once_its_packet_is_acked() {
        // Before the ack, `collect_outgoing_messages` re-queues the message for
        // retransmission; after it, there is nothing left to resend. That
        // transition is the whole purpose of `packet_to_message_map`.
        let kinds = directional_kinds();
        let messages = message_kinds();
        let mut manager = MessageManager::new(HostType::Client, &kinds);
        manager.send_message(
            &messages,
            &mut FakeEntityConverter,
            &ChannelKind::of::<ToServer>(),
            tagged(1, 4),
        );

        let now = Instant::now();
        manager.collect_outgoing_messages(&now, &200.0);
        let mut writer = BitWriter::new();
        let mut has_written = false;
        manager.write_messages(
            &kinds,
            &messages,
            &mut FakeEntityConverter,
            &mut writer,
            7,
            &mut has_written,
        );
        assert!(has_written, "the message should have been written");

        manager.notify_packet_delivered(7);

        // Advance well past any resend timeout.
        let mut later = Instant::now();
        later.add_millis(10_000);
        manager.collect_outgoing_messages(&later, &200.0);
        assert!(
            !manager.has_outgoing_messages(),
            "an acked reliable message must not be re-queued for retransmission"
        );
    }

    #[test]
    fn an_unacked_reliable_message_is_requeued_for_retransmission() {
        // The counterpart to the test above: without the ack, the same elapsed
        // time DOES bring the message back. Without this, that test would pass
        // just as well if `collect_outgoing_messages` did nothing at all.
        let kinds = directional_kinds();
        let messages = message_kinds();
        let mut manager = MessageManager::new(HostType::Client, &kinds);
        manager.send_message(
            &messages,
            &mut FakeEntityConverter,
            &ChannelKind::of::<ToServer>(),
            tagged(1, 4),
        );

        let now = Instant::now();
        manager.collect_outgoing_messages(&now, &200.0);
        let mut writer = BitWriter::new();
        let mut has_written = false;
        manager.write_messages(
            &kinds,
            &messages,
            &mut FakeEntityConverter,
            &mut writer,
            7,
            &mut has_written,
        );
        assert!(
            !manager.has_outgoing_messages(),
            "the queue drains on write"
        );

        let mut later = Instant::now();
        later.add_millis(10_000);
        manager.collect_outgoing_messages(&later, &200.0);
        assert!(
            manager.has_outgoing_messages(),
            "an unacked reliable message must come back for another attempt"
        );
    }

    // -- requests and responses ---------------------------------------------

    #[test]
    fn only_bidirectional_reliable_channels_carry_requests() {
        // `can_request_and_respond` is reliable AND both directions. A manager
        // built from channels that satisfy neither must report nothing rather
        // than reaching into a sender that would panic on the attempt.
        let mut kinds = ChannelKinds::new();
        kinds.add_channel::<Unreliable>(unreliable(ChannelDirection::Bidirectional));
        kinds.add_channel::<ToServer>(reliable(ChannelDirection::ClientToServer));

        let mut manager = MessageManager::new(HostType::Client, &kinds);
        let (requests, responses) = manager.receive_requests_and_responses();
        assert!(requests.is_empty());
        assert!(responses.is_empty());
    }

    #[test]
    fn the_fragmentation_gate_opens_one_bit_past_the_limit() {
        // Exactly at the limit is a single fragment's worth, so fragmenting it
        // would only add header overhead -- and an unreliable channel, which
        // refuses anything that would need fragmenting, must still carry it.
        // One bit more must be refused.
        let mut kinds = ChannelKinds::new();
        kinds.add_channel::<Unreliable>(unreliable(ChannelDirection::Bidirectional));
        let mut manager = MessageManager::new(HostType::Client, &kinds);
        let channel = ChannelKind::of::<Unreliable>();

        assert_eq!(
            exact(FRAGMENTATION_LIMIT_BITS).bit_length(&message_kinds(), &mut FakeEntityConverter),
            FRAGMENTATION_LIMIT_BITS,
            "the fixture must land ON the boundary for this test to mean anything"
        );

        assert!(
            manager.send_message(
                &message_kinds(),
                &mut FakeEntityConverter,
                &channel,
                exact(FRAGMENTATION_LIMIT_BITS)
            ),
            "a message of exactly the limit is not fragmented"
        );
        assert!(
            !manager.send_message(
                &message_kinds(),
                &mut FakeEntityConverter,
                &channel,
                exact(FRAGMENTATION_LIMIT_BITS + 1)
            ),
            "one bit past the limit needs fragmenting, which unreliable refuses"
        );
    }

    #[test]
    fn a_request_refused_by_a_full_queue_is_reported_as_dropped() {
        // send_request forwards the channel's own verdict. A caller that treats
        // it as infallible would silently lose the request and then wait
        // forever for a response that was never sent.
        let mut kinds = ChannelKinds::new();
        kinds.add_channel::<BothWays>(ChannelSettings::new(
            ChannelMode::OrderedReliable(ReliableSettings {
                rtt_resend_factor: 1.5,
                max_queue_depth: Some(1),
            }),
            ChannelDirection::Bidirectional,
        ));
        let mut manager = MessageManager::new(HostType::Client, &kinds);
        let channel = ChannelKind::of::<BothWays>();

        assert!(
            manager.send_request(
                &message_kinds(),
                &mut FakeEntityConverter,
                &channel,
                GlobalRequestId::new(0),
                ping(1),
            ),
            "the first request fits the one-deep queue"
        );
        assert!(
            !manager.send_request(
                &message_kinds(),
                &mut FakeEntityConverter,
                &channel,
                GlobalRequestId::new(1),
                ping(2),
            ),
            "the second must be refused, not silently swallowed"
        );
    }

    #[test]
    fn a_request_and_its_response_cross_the_wire_and_come_back_paired() {
        // The manager is the only thing that maps a response back onto the
        // GlobalRequestId the caller is waiting on: the wire carries a local,
        // per-channel id. If that mapping is dropped -- or the request buffer
        // reported empty -- the caller waits forever on a request that was
        // in fact answered.
        let mut kinds = ChannelKinds::new();
        kinds.add_channel::<BothWays>(reliable(ChannelDirection::Bidirectional));
        let messages = message_kinds();
        let channel = ChannelKind::of::<BothWays>();
        let mut client = MessageManager::new(HostType::Client, &kinds);
        let mut server = MessageManager::new(HostType::Server, &kinds);

        assert!(client.send_request(
            &messages,
            &mut FakeEntityConverter,
            &channel,
            GlobalRequestId::new(7),
            ping(1),
        ));
        deliver(&mut client, &mut server, &kinds, &messages);

        let (requests, responses) = server.receive_requests_and_responses();
        assert!(responses.is_empty(), "the server answered nothing yet");
        assert_eq!(requests.len(), 1, "the request must be reported, once");
        let (request_channel, mut on_channel) = requests.into_iter().next().unwrap();
        assert_eq!(request_channel, channel);
        assert_eq!(on_channel.len(), 1);
        let (response_id, request) = on_channel.remove(0);
        assert_eq!(
            request.to_boxed_any().downcast::<Ping>().unwrap().0,
            1,
            "and it must carry the payload that was sent"
        );

        assert!(server.send_response(
            &messages,
            &mut FakeEntityConverter,
            &channel,
            response_id,
            ping(2),
        ));
        deliver(&mut server, &mut client, &kinds, &messages);

        let (requests, responses) = client.receive_requests_and_responses();
        assert!(requests.is_empty(), "the client was asked nothing");
        assert_eq!(responses.len(), 1);
        let (global_request_id, response) = responses.into_iter().next().unwrap();
        assert_eq!(
            global_request_id,
            GlobalRequestId::new(7),
            "the response must resolve to the id the caller is waiting on"
        );
        assert_eq!(response.to_boxed_any().downcast::<Ping>().unwrap().0, 2);
    }

    #[test]
    fn a_response_refused_by_a_full_queue_is_reported_as_dropped() {
        // Same contract as send_request: the channel's verdict is forwarded, so
        // a caller can retry rather than leave the peer waiting.
        let mut kinds = ChannelKinds::new();
        kinds.add_channel::<BothWays>(ChannelSettings::new(
            ChannelMode::OrderedReliable(ReliableSettings {
                rtt_resend_factor: 1.5,
                max_queue_depth: Some(1),
            }),
            ChannelDirection::Bidirectional,
        ));
        let mut manager = MessageManager::new(HostType::Client, &kinds);
        let channel = ChannelKind::of::<BothWays>();
        let messages = message_kinds();

        assert!(
            manager.send_response(
                &messages,
                &mut FakeEntityConverter,
                &channel,
                LocalRequestId::from(0u16).receive_from_remote(),
                ping(1),
            ),
            "the first response fits the one-deep queue"
        );
        assert!(
            !manager.send_response(
                &messages,
                &mut FakeEntityConverter,
                &channel,
                LocalRequestId::from(1u16).receive_from_remote(),
                ping(2),
            ),
            "the second must be refused, not silently swallowed"
        );
    }
}
