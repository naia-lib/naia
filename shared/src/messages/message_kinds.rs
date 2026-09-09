use std::{any::TypeId, collections::HashMap};

use naia_serde::{BitReader, BitWrite, Serde, SerdeErr};

use crate::{
    LocalEntityAndGlobalEntityConverter, Message, MessageBuilder, MessageContainer, Request,
};

type NetId = u16;

/// Wire encoding for `MessageKind` NetIds: a fixed-width raw bit field
/// whose width is `ceil(log2(N))` for the protocol's registered message
/// count. Both ends share registration order, so both compute the same
/// width. See `world::component::component_kinds` for the matching
/// rationale on the component side — same logic, same shape.
fn bit_width_for_kind_count(count: NetId) -> u8 {
    if count < 2 {
        0
    } else {
        (count as u32).next_power_of_two().trailing_zeros() as u8
    }
}

/// MessageKind - should be one unique value for each type of Message
#[derive(Eq, Hash, Copy, Clone, PartialEq, Debug)]
pub struct MessageKind {
    type_id: TypeId,
}

impl MessageKind {
    /// Returns the `MessageKind` corresponding to the type `M`.
    pub fn of<M: Message>() -> Self {
        Self {
            type_id: TypeId::of::<M>(),
        }
    }

    /// Serializes this kind's compact net-ID into `writer` using the bit-width in `message_kinds`.
    pub fn ser(&self, message_kinds: &MessageKinds, writer: &mut dyn BitWrite) {
        let net_id = message_kinds.kind_to_net_id(self);
        let bits = message_kinds.kind_bit_width;
        for i in 0..bits {
            writer.write_bit((net_id >> i) & 1 != 0);
        }
    }

    /// Deserializes a `MessageKind` from `reader` using the bit-width in `message_kinds`.
    pub fn de(message_kinds: &MessageKinds, reader: &mut BitReader) -> Result<Self, SerdeErr> {
        let bits = message_kinds.kind_bit_width;
        let mut net_id: NetId = 0;
        for i in 0..bits {
            if bool::de(reader)? {
                net_id |= 1 << i;
            }
        }
        message_kinds.net_id_to_kind(&net_id)
    }
}

/// Registry mapping `Message` types to compact wire net-IDs and their deserializers.
pub struct MessageKinds {
    current_net_id: NetId,
    /// Number of bits needed to encode any registered NetId — recomputed
    /// on every `add_message`. Read directly by `MessageKind::ser`/`de`
    /// on the hot path.
    kind_bit_width: u8,
    kind_map: HashMap<MessageKind, (NetId, Box<dyn MessageBuilder>, String)>,
    net_id_map: HashMap<NetId, MessageKind>,
    /// Canonical domain descriptor per registered message, keyed by kind.
    /// Stored at registration from `M::wire_schema()` so the fingerprint can
    /// compare field layout without re-walking the types.
    descriptors: HashMap<MessageKind, Vec<u8>>,
    /// Request→response pairs in registration order, as wire net-ID pairs.
    /// Recorded only by [`add_request`](Self::add_request): the two internal
    /// envelope registrations in `Protocol::default` are plain messages, not
    /// pairs. A response net-ID that never appears here is a message that
    /// only ever travels standalone.
    request_pairs: Vec<(NetId, NetId)>,
}

impl Clone for MessageKinds {
    fn clone(&self) -> Self {
        let current_net_id = self.current_net_id;
        let kind_bit_width = self.kind_bit_width;
        let net_id_map = self.net_id_map.clone();

        let mut kind_map = HashMap::new();
        for (key, value) in self.kind_map.iter() {
            kind_map.insert(*key, (value.0, value.1.box_clone(), value.2.clone()));
        }

        Self {
            current_net_id,
            kind_bit_width,
            kind_map,
            net_id_map,
            descriptors: self.descriptors.clone(),
            request_pairs: self.request_pairs.clone(),
        }
    }
}

impl Default for MessageKinds {
    fn default() -> Self {
        Self::new()
    }
}

impl MessageKinds {
    /// Creates an empty `MessageKinds` registry.
    pub fn new() -> Self {
        Self {
            current_net_id: 0,
            kind_bit_width: 0,
            kind_map: HashMap::new(),
            net_id_map: HashMap::new(),
            descriptors: HashMap::new(),
            request_pairs: Vec::new(),
        }
    }

    /// Registers message type `M`, assigning it the next sequential net-ID.
    pub fn add_message<M: Message>(&mut self) {
        let message_kind = MessageKind::of::<M>();

        let net_id = self.current_net_id;
        self.kind_map.insert(
            message_kind,
            (net_id, M::create_builder(), M::protocol_name().to_string()),
        );
        self.net_id_map.insert(net_id, message_kind);
        self.descriptors.insert(message_kind, M::wire_schema());
        debug_assert!(
            self.current_net_id < NetId::MAX,
            "MessageKinds NetId overflow — too many message types registered (max {})",
            NetId::MAX
        );
        self.current_net_id += 1;
        self.kind_bit_width = bit_width_for_kind_count(self.current_net_id);
    }

    /// Registers request type `Q` and its associated response type, recording
    /// the request→response pairing as wire net-IDs.
    ///
    /// Net-ID assignment is identical to two sequential `add_message` calls
    /// (`Q` first, then `Q::Response`), so routing `Protocol::add_request`
    /// through here changes nothing on the wire — it only preserves the
    /// pairing that two bare calls would discard.
    pub fn add_request<Q: Request>(&mut self) {
        self.add_message::<Q>();
        self.add_message::<Q::Response>();
        let request_net_id = self.kind_to_net_id(&MessageKind::of::<Q>());
        let response_net_id = self.kind_to_net_id(&MessageKind::of::<Q::Response>());
        self.request_pairs.push((request_net_id, response_net_id));
    }

    /// Request→response pairs in registration order, as
    /// `(request_net_id, response_net_id)` wire net-ID pairs.
    pub fn request_response_pairs(&self) -> &[(NetId, NetId)] {
        &self.request_pairs
    }

    /// Returns every registered message's canonical domain descriptor in
    /// **wire net-ID order**, as `(net_id, descriptor_bytes)`.
    ///
    /// Same traversal contract as [`schema_entries`](Self::schema_entries):
    /// net-IDs are dense registration ordinals, so the walk covers the whole
    /// net-ID space and `HashMap` iteration order never leaks into the
    /// result.
    pub fn schema_descriptor_entries(&self) -> Vec<(NetId, Vec<u8>)> {
        let mut output = Vec::with_capacity(self.current_net_id as usize);
        for net_id in 0..self.current_net_id {
            let kind = self
                .net_id_map
                .get(&net_id)
                .expect("MessageKinds net-ID space must be dense");
            let descriptor = self
                .descriptors
                .get(kind)
                .expect("every registered MessageKind must have a descriptor");
            output.push((net_id, descriptor.clone()));
        }
        output
    }

    /// Bit width of every encoded `MessageKind` in this registry. Used by
    /// derived `Message::bit_length` impls to size the kind-tag prefix.
    pub fn kind_bit_length(&self) -> u32 {
        self.kind_bit_width as u32
    }

    /// Reads a message kind tag then deserializes and returns the message payload from `reader`.
    pub fn read(
        &self,
        reader: &mut BitReader,
        converter: &dyn LocalEntityAndGlobalEntityConverter,
    ) -> Result<MessageContainer, SerdeErr> {
        let message_kind: MessageKind = MessageKind::de(self, reader)?;
        self.kind_to_builder(&message_kind).read(reader, converter)
    }

    /// Resolves a net-ID read from the wire into a registered `MessageKind`.
    ///
    /// The net-ID comes from a remote peer, so an unregistered value is a
    /// malformed packet rather than a local programming error: return an error
    /// and let the caller drop the packet.
    fn net_id_to_kind(&self, net_id: &NetId) -> Result<MessageKind, SerdeErr> {
        self.net_id_map.get(net_id).copied().ok_or(SerdeErr)
    }

    fn kind_to_net_id(&self, message_kind: &MessageKind) -> NetId {
        self.kind_map
            .get(message_kind)
            .expect("Must properly initialize Message with Protocol via `add_message()` function!")
            .0
    }

    fn kind_to_builder(&self, message_kind: &MessageKind) -> &dyn MessageBuilder {
        self.kind_map
            .get(message_kind)
            .expect("Must properly initialize Message with Protocol via `add_message()` function!")
            .1
            .as_ref()
    }

    /// Returns every registered message in **wire net-ID order**, as
    /// `(net_id, protocol_name)`.
    ///
    /// The protocol fingerprint is built from this rather than from
    /// [`all_names`](Self::all_names): net-IDs are registration ordinals that
    /// travel on the wire, so hashing sorted names would leave a reordering
    /// that renumbers every message undetectable. Walking the net-ID space
    /// also keeps `HashMap` iteration order out of the result, which is what
    /// makes the fingerprint reproducible across processes.
    ///
    /// Net-IDs are dense by construction, so a gap is a broken invariant.
    pub fn schema_entries(&self) -> Vec<(NetId, String)> {
        let mut output = Vec::with_capacity(self.current_net_id as usize);
        for net_id in 0..self.current_net_id {
            let kind = self
                .net_id_map
                .get(&net_id)
                .expect("MessageKinds net-ID space must be dense");
            let (_, _, name) = self
                .kind_map
                .get(kind)
                .expect("every registered MessageKind must have a name");
            output.push((net_id, name.clone()));
        }
        output
    }

    /// Returns a sorted list of all registered message protocol names.
    pub fn all_names(&self) -> Vec<String> {
        let mut output = Vec::new();
        for (_, _, name) in self.kind_map.values() {
            output.push(name.clone());
        }
        output.sort();
        output
    }
}

#[cfg(test)]
mod tests {
    use naia_serde::BitReader;

    use crate::{MessageKind, MessageKinds};

    /// A net_id with no registered `Message` behind it is something a remote peer
    /// can put on the wire, so decoding it must return an error the caller can
    /// drop the packet on -- not panic and take the whole process down.
    #[test]
    fn unregistered_net_id_errors_instead_of_panicking() {
        let kinds = MessageKinds::new();
        let bytes = [0u8; 4];
        let mut reader = BitReader::new(&bytes);
        assert!(MessageKind::de(&kinds, &mut reader).is_err());
    }
}
