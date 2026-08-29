use std::ops::{Deref, DerefMut};

use naia_serde::{BitReader, BitWrite, BitWriter, Serde, SerdeErr};

use crate::world::{
    component::property_mutate::PropertyMutator, delegation::auth_channel::EntityAuthAccessor,
};

#[derive(Clone)]
enum PropertyImpl<T: Serde> {
    HostOwned(HostOwnedProperty<T>),
    RemoteOwned(RemoteOwnedProperty<T>),
    RemotePublic(RemotePublicProperty<T>),
    Delegated(DelegatedProperty<T>),
    Local(LocalProperty<T>),
}

impl<T: Serde> PropertyImpl<T> {
    fn name(&self) -> &str {
        match self {
            PropertyImpl::HostOwned(_) => "HostOwned",
            PropertyImpl::RemoteOwned(_) => "RemoteOwned",
            PropertyImpl::RemotePublic(_) => "RemotePublic",
            PropertyImpl::Delegated(_) => "Delegated",
            PropertyImpl::Local(_) => "Local",
        }
    }
}

/// A Property of an Component/Message, that contains data
/// which must be tracked for updates
#[derive(Clone)]
pub struct Property<T: Serde> {
    inner: PropertyImpl<T>,
}

// should be shared
impl<T: Serde> Property<T> {
    /// Create a new Local Property
    pub fn new_local(value: T) -> Self {
        Self {
            inner: PropertyImpl::Local(LocalProperty::new(value)),
        }
    }

    /// Create a new host-owned Property
    pub fn host_owned(value: T, mutator_index: u8) -> Self {
        Self {
            inner: PropertyImpl::HostOwned(HostOwnedProperty::new(value, mutator_index)),
        }
    }

    /// Create a new host-owned Property for an *immutable* (seed-only)
    /// component. Identical to [`Self::host_owned`] except its `mutate()`
    /// tolerates the permanent absence of a `PropertyMutator`: immutable
    /// components are deliberately never registered for diff-tracking (see
    /// `host_world_manager::init_entity_send_host_commands` /
    /// `global_world_manager::insert_component_diff_handler`), so they never
    /// receive a mutator — yet the host sim may freely mutate the value every
    /// tick. Each new observer is seeded with the *current* value (re-read at
    /// spawn/insert), and existing observers never see an update. This is the
    /// value-carrying seed-only replication primitive.
    pub fn immutable_host_owned(value: T, mutator_index: u8) -> Self {
        Self {
            inner: PropertyImpl::HostOwned(HostOwnedProperty::new_immutable(value, mutator_index)),
        }
    }

    /// Given a cursor into incoming packet data, initializes the Property with
    /// the synced value
    pub fn new_read(reader: &mut BitReader) -> Result<Self, SerdeErr> {
        let inner_value = Self::read_inner(reader)?;

        Ok(Self {
            inner: PropertyImpl::RemoteOwned(RemoteOwnedProperty::new(inner_value)),
        })
    }

    /// Set an PropertyMutator to track changes to the Property
    pub fn set_mutator(&mut self, mutator: &PropertyMutator) {
        match &mut self.inner {
            PropertyImpl::HostOwned(inner) => {
                inner.set_mutator(mutator);
            }
            PropertyImpl::RemoteOwned(_) | PropertyImpl::RemotePublic(_) => {
                panic!("Remote Property should never call set_mutator().");
            }
            PropertyImpl::Delegated(_) => {
                panic!("Delegated Property should never call set_mutator().");
            }
            PropertyImpl::Local(_) => {
                panic!("Local Property should never have a mutator.");
            }
        }
    }

    // Serialization / deserialization

    /// Writes contained value into outgoing byte stream
    pub fn write(&self, writer: &mut dyn BitWrite) {
        match &self.inner {
            PropertyImpl::HostOwned(inner) => {
                inner.write(writer);
            }
            PropertyImpl::RemoteOwned(_) => {
                panic!("Remote Private Property should never be written.");
            }
            PropertyImpl::RemotePublic(inner) => {
                inner.write(writer);
            }
            PropertyImpl::Local(_) => {
                panic!("Local Property should never be written.");
            }
            PropertyImpl::Delegated(inner) => {
                inner.write(writer);
            }
        }
    }

    /// Reads from a stream and immediately writes to a stream
    /// Used to buffer updates for later
    pub fn read_write(reader: &mut BitReader, writer: &mut BitWriter) -> Result<(), SerdeErr> {
        T::de(reader)?.ser(writer);
        Ok(())
    }

    /// Given a cursor into incoming packet data, updates the Property with the
    /// synced value
    pub fn read(&mut self, reader: &mut BitReader) -> Result<(), SerdeErr> {
        match &mut self.inner {
            PropertyImpl::HostOwned(_) => {
                panic!("Host Property should never read.");
            }
            PropertyImpl::RemoteOwned(inner) => {
                inner.read(reader)?;
            }
            PropertyImpl::RemotePublic(inner) => {
                inner.read(reader)?;
            }
            PropertyImpl::Local(_) => {
                panic!("Local Property should never read.");
            }
            PropertyImpl::Delegated(inner) => {
                inner.read(reader)?;
            }
        }
        Ok(())
    }

    fn read_inner(reader: &mut BitReader) -> Result<T, SerdeErr> {
        T::de(reader)
    }

    // Comparison

    fn inner(&self) -> &T {
        match &self.inner {
            PropertyImpl::HostOwned(inner) => &inner.inner,
            PropertyImpl::RemoteOwned(inner) => &inner.inner,
            PropertyImpl::RemotePublic(inner) => &inner.inner,
            PropertyImpl::Local(inner) => &inner.inner,
            PropertyImpl::Delegated(inner) => &inner.inner,
        }
    }

    /// Compare to another property
    pub fn equals(&self, other: &Self) -> bool {
        self.inner() == other.inner()
    }

    /// Set value to the value of another Property, queues for update if value
    /// changes
    pub fn mirror(&mut self, other: &Self) {
        let other_inner = other.inner();
        match &mut self.inner {
            PropertyImpl::HostOwned(inner) => {
                inner.mirror(other_inner);
            }
            PropertyImpl::RemoteOwned(_) | PropertyImpl::RemotePublic(_) => {
                panic!("Remote Property should never be set manually.");
            }
            PropertyImpl::Delegated(inner) => {
                inner.mirror(other_inner);
            }
            PropertyImpl::Local(inner) => {
                inner.mirror(other_inner);
            }
        }
    }

    /// Migrate Remote Property to Public version
    pub fn remote_publish(&mut self, mutator_index: u8, mutator: &PropertyMutator) {
        match &mut self.inner {
            PropertyImpl::HostOwned(_) => {
                panic!("Host Property should never be made public.");
            }
            PropertyImpl::RemoteOwned(inner) => {
                let inner_value = inner.inner.clone();
                self.inner = PropertyImpl::RemotePublic(RemotePublicProperty::new(
                    inner_value,
                    mutator_index,
                    mutator,
                ));
            }
            PropertyImpl::RemotePublic(_) => {
                panic!("Remote Property should never be made public twice.");
            }
            PropertyImpl::Local(_) => {
                panic!("Local Property should never be made public.");
            }
            PropertyImpl::Delegated(_) => {
                panic!("Delegated Property should never be made public.");
            }
        }
    }

    /// Migrate Remote Property to Private version
    pub fn remote_unpublish(&mut self) {
        match &mut self.inner {
            PropertyImpl::HostOwned(_) => {
                panic!("Host Property should never be unpublished.");
            }
            PropertyImpl::RemoteOwned(_) => {
                panic!("Private Remote Property should never be unpublished.");
            }
            PropertyImpl::RemotePublic(inner) => {
                let inner_value = inner.inner.clone();
                self.inner = PropertyImpl::RemoteOwned(RemoteOwnedProperty::new(inner_value));
            }
            PropertyImpl::Local(_) => {
                panic!("Local Property should never be unpublished.");
            }
            PropertyImpl::Delegated(_) => {
                panic!("Delegated Property should never be unpublished.");
            }
        }
    }

    /// Migrate Property to Delegated version
    pub fn enable_delegation(
        &mut self,
        accessor: &EntityAuthAccessor,
        mutator_opt: Option<(u8, &PropertyMutator)>,
    ) {
        let value = self.inner().clone();

        let (mutator_index, mutator) = {
            if let Some((mutator_index, mutator)) = mutator_opt {
                match &mut self.inner {
                    PropertyImpl::RemoteOwned(_) => (mutator_index, mutator),
                    PropertyImpl::Local(_)
                    | PropertyImpl::RemotePublic(_)
                    | PropertyImpl::HostOwned(_)
                    | PropertyImpl::Delegated(_) => {
                        panic!(
                            "Property of type `{:?}` should never enable delegation this way",
                            self.inner.name()
                        );
                    }
                }
            } else {
                match &mut self.inner {
                    PropertyImpl::HostOwned(inner) => (
                        inner.index,
                        inner
                            .mutator
                            .as_ref()
                            .expect("should have a mutator by now"),
                    ),
                    PropertyImpl::RemotePublic(inner) => (inner.index, &inner.mutator),
                    PropertyImpl::RemoteOwned(_)
                    | PropertyImpl::Delegated(_)
                    | PropertyImpl::Local(_) => {
                        panic!(
                            "Property of type `{:?}` should never enable delegation this way",
                            self.inner.name()
                        );
                    }
                }
            }
        };

        self.inner = PropertyImpl::Delegated(DelegatedProperty::new(
            value,
            accessor,
            mutator,
            mutator_index,
        ));
    }

    /// Migrate Delegated Property to Host-Owned (Public) version
    pub fn disable_delegation(&mut self) {
        match &mut self.inner {
            PropertyImpl::HostOwned(_) => {
                panic!("Host Property should never disable delegation.");
            }
            PropertyImpl::RemoteOwned(_) => {
                panic!("Private Remote Property should never disable delegation.");
            }
            PropertyImpl::RemotePublic(_) => {
                panic!("Public Remote Property should never disable delegation.");
            }
            PropertyImpl::Local(_) => {
                panic!("Local Property should never disable delegation.");
            }
            PropertyImpl::Delegated(inner) => {
                let inner_value = inner.inner.clone();
                let mut new_inner = HostOwnedProperty::new(inner_value, inner.index);
                new_inner.set_mutator(&inner.mutator);
                self.inner = PropertyImpl::HostOwned(new_inner);
            }
        }
    }

    /// Migrate Host Property to Local version
    pub fn localize(&mut self) {
        match &mut self.inner {
            PropertyImpl::HostOwned(inner) => {
                let inner_value = inner.inner.clone();
                self.inner = PropertyImpl::Local(LocalProperty::new(inner_value));
            }
            PropertyImpl::RemoteOwned(_) | PropertyImpl::RemotePublic(_) => {
                panic!("Remote Property should never be made local.");
            }
            PropertyImpl::Local(_) => {
                panic!("Local Property should never be made local twice.");
            }
            PropertyImpl::Delegated(_) => {
                panic!("Delegated Property should never be made local.");
            }
        }
    }
}

// It could be argued that Property here is a type of smart-pointer,
// but honestly this is mainly for the convenience of type coercion
impl<T: Serde> Deref for Property<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        self.inner()
    }
}

impl<T: Serde> DerefMut for Property<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        // Just assume inner value will be changed, queue for update
        match &mut self.inner {
            PropertyImpl::HostOwned(inner) => {
                inner.mutate();
                &mut inner.inner
            }
            PropertyImpl::Delegated(inner) => {
                inner.mutate();
                &mut inner.inner
            }
            PropertyImpl::RemoteOwned(inner) => &mut inner.inner,
            PropertyImpl::RemotePublic(inner) => &mut inner.inner,
            PropertyImpl::Local(inner) => &mut inner.inner,
        }
    }
}

#[derive(Clone)]
pub struct HostOwnedProperty<T: Serde> {
    inner: T,
    mutator: Option<PropertyMutator>,
    index: u8,
    /// `true` for Properties of an immutable (seed-only) component, which are
    /// never diff-tracked and thus never receive a mutator. Tells `mutate()`
    /// that a missing mutator is by-design, not the bug the warning guards
    /// against. Mutable components leave this `false`.
    immutable: bool,
}

impl<T: Serde> HostOwnedProperty<T> {
    /// Create a new HostOwnedProperty
    pub fn new(value: T, mutator_index: u8) -> Self {
        Self {
            inner: value,
            mutator: None,
            index: mutator_index,
            immutable: false,
        }
    }

    /// Create a new HostOwnedProperty for an immutable (seed-only) component —
    /// see [`Property::immutable_host_owned`].
    pub fn new_immutable(value: T, mutator_index: u8) -> Self {
        Self {
            inner: value,
            mutator: None,
            index: mutator_index,
            immutable: true,
        }
    }

    pub fn set_mutator(&mut self, mutator: &PropertyMutator) {
        self.mutator = Some(mutator.clone_new());
    }

    pub fn write(&self, writer: &mut dyn BitWrite) {
        self.inner.ser(writer);
    }

    pub fn mirror(&mut self, other: &T) {
        self.mutate();
        self.inner = other.clone();
    }

    pub fn mutate(&mut self) {
        let Some(mutator) = &mut self.mutator else {
            // Immutable (seed-only) components are never diff-tracked, so the
            // permanent absence of a mutator is expected — the host sim mutates
            // the value every tick and each new observer is seeded with the
            // current value at spawn. For those, mutation is a legitimate no-op.
            if self.immutable {
                return;
            }
            // A *mutable* HostOwned Property reaching here was mutated before
            // its replication mutator was installed — an invariant violation
            // that silently drops the change from every observer's diff. This
            // used to `warn!`-and-continue, which let the bug rot. The two
            // legitimate ways to mutate a mutatorless Property are: build the
            // finished value in one shot via `new_complete` (no mutation), or
            // `localize()` it first (converts to a mutate-freely `Local`).
            panic!(
                "mutable HostOwned Property mutated before its mutator was \
                 installed — construct it complete (no pre-registration \
                 mutation) or `localize()` it before mutating"
            );
        };
        mutator.mutate(self.index);
    }
}

#[derive(Clone)]
pub struct LocalProperty<T: Serde> {
    inner: T,
}

impl<T: Serde> LocalProperty<T> {
    /// Create a new LocalProperty
    pub fn new(value: T) -> Self {
        Self { inner: value }
    }

    pub fn mirror(&mut self, other: &T) {
        self.inner = other.clone();
    }
}

#[derive(Clone)]
pub struct RemoteOwnedProperty<T: Serde> {
    inner: T,
}

impl<T: Serde> RemoteOwnedProperty<T> {
    /// Create a new RemoteOwnedProperty
    pub fn new(value: T) -> Self {
        Self { inner: value }
    }

    pub fn read(&mut self, reader: &mut BitReader) -> Result<(), SerdeErr> {
        self.inner = Property::read_inner(reader)?;
        Ok(())
    }
}

#[derive(Clone)]
pub struct RemotePublicProperty<T: Serde> {
    inner: T,
    mutator: PropertyMutator,
    index: u8,
}

impl<T: Serde> RemotePublicProperty<T> {
    /// Create a new RemotePublicProperty
    pub fn new(value: T, mutator_index: u8, mutator: &PropertyMutator) -> Self {
        Self {
            inner: value,
            mutator: mutator.clone_new(),
            index: mutator_index,
        }
    }

    pub fn read(&mut self, reader: &mut BitReader) -> Result<(), SerdeErr> {
        self.inner = Property::read_inner(reader)?;
        self.mutate();
        Ok(())
    }

    pub fn write(&self, writer: &mut dyn BitWrite) {
        self.inner.ser(writer);
    }

    fn mutate(&mut self) {
        let _success = self.mutator.mutate(self.index);
    }
}

#[derive(Clone)]
pub struct DelegatedProperty<T: Serde> {
    inner: T,
    auth_accessor: EntityAuthAccessor,
    mutator: PropertyMutator,
    index: u8,
}

impl<T: Serde> DelegatedProperty<T> {
    /// Create a new DelegatedProperty
    pub fn new(
        value: T,
        auth_accessor: &EntityAuthAccessor,
        mutator: &PropertyMutator,
        index: u8,
    ) -> Self {
        Self {
            inner: value,
            auth_accessor: auth_accessor.clone(),
            mutator: mutator.clone_new(),
            index,
        }
    }

    pub fn read(&mut self, reader: &mut BitReader) -> Result<(), SerdeErr> {
        let value = Property::read_inner(reader)?;

        if self.can_read() {
            self.inner = value;
            if self.can_mutate() {
                self.mutate();
            }
        }

        Ok(())
    }

    pub fn write(&self, writer: &mut dyn BitWrite) {
        if !self.can_write() {
            panic!("Must have Authority over Entity before performing this operation. Current Authority: {:?}", self.auth_accessor.auth_status());
        }
        self.inner.ser(writer);
    }

    pub fn mirror(&mut self, other: &T) {
        self.mutate();
        self.inner = other.clone();
    }

    fn mutate(&mut self) {
        if !self.can_mutate() {
            panic!("Must request authority to mutate a Delegated Property.");
        }
        let _success = self.mutator.mutate(self.index);
    }

    fn can_mutate(&self) -> bool {
        self.auth_accessor.auth_status().can_mutate()
    }

    fn can_read(&self) -> bool {
        self.auth_accessor.auth_status().can_read()
    }

    fn can_write(&self) -> bool {
        self.auth_accessor.auth_status().can_write()
    }
}

#[cfg(test)]
mod delegated_write_auth_tests {
    //! Root 2 of the delegated-authority family found by the Cyberlith NPA
    //! promotion gate: a dirty update queued while a host held authority can
    //! reach `send_packets` after that authority is gone, and
    //! `DelegatedProperty::write` then panics.
    //!
    //! The fix is a guard in `WorldWriter::write_updates` that drops such a
    //! planned update instead of serializing it, in the same spirit as the
    //! despawn-race guards beside it. That guard's condition is
    //! `!auth_status.can_write()`, and `write` panics on that same predicate,
    //! so no test can meaningfully assert the two agree -- they are the same
    //! call. What these tests pin instead is the authority *table* the guard
    //! reads: the client state the NPA backtrace reported must stay
    //! non-writable, and the server must stay writable in every state so the
    //! guard remains free on the server path.

    use super::*;
    use crate::{
        world::delegation::{
            auth_channel::EntityAuthChannel, entity_auth_status::EntityAuthStatus,
        },
        HostType, PropertyMutate, PropertyMutator,
    };

    #[derive(Clone)]
    struct NoopMutator;

    impl PropertyMutate for NoopMutator {
        fn mutate(&mut self, _property_index: u8) -> bool {
            true
        }
    }

    const ALL_STATUSES: [EntityAuthStatus; 5] = [
        EntityAuthStatus::Available,
        EntityAuthStatus::Requested,
        EntityAuthStatus::Granted,
        EntityAuthStatus::Releasing,
        EntityAuthStatus::Denied,
    ];

    fn property_at(host_type: HostType, status: EntityAuthStatus) -> DelegatedProperty<String> {
        let (auth_mutator, accessor) = EntityAuthChannel::new_channel(host_type);
        auth_mutator.set_auth_status(status);
        let prop_mutator = PropertyMutator::new(NoopMutator);
        DelegatedProperty::new("value".to_string(), &accessor, &prop_mutator, 0)
    }

    /// The specific state the NPA repro reported: `Client/Available`.
    #[test]
    fn a_client_without_authority_cannot_write() {
        let property = property_at(HostType::Client, EntityAuthStatus::Available);
        assert!(
            !property.can_write(),
            "Client/Available is the state the NPA backtrace reported at \
             property.rs:541; the send guard must drop its queued updates",
        );
    }

    /// The server is never gated, so the guard costs it nothing.
    #[test]
    fn the_server_can_always_write() {
        for status in ALL_STATUSES {
            assert!(
                property_at(HostType::Server, status).can_write(),
                "server must remain writable at {status:?}",
            );
        }
    }

    /// Runs `body`, returning the panic message if it panicked. Silences the
    /// default hook so an expected panic does not spam the test output.
    fn panic_message_of(body: impl FnOnce()) -> Option<String> {
        let previous = std::panic::take_hook();
        std::panic::set_hook(Box::new(|_| {}));
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(body));
        std::panic::set_hook(previous);
        result.err().map(|payload| {
            payload
                .downcast_ref::<String>()
                .cloned()
                .or_else(|| payload.downcast_ref::<&str>().map(|s| (*s).to_string()))
                .unwrap_or_else(|| "<non-string panic payload>".to_string())
        })
    }

    /// Audit of `DelegatedProperty::mirror` (call-site audit, item 2).
    ///
    /// `mirror` calls `mutate()` unconditionally -- the same shape as root 1,
    /// where `read_none`/`read_some` did. It is *not* the same bug, because
    /// every in-tree caller is pre-gated:
    ///
    /// - `server/src/world/entity_mut.rs:170` and
    ///   `server/src/server/world_server.rs:2754` run on the server, and
    ///   `can_mutate()` is true for every server auth status.
    /// - `client/src/client.rs:insert_component` checks
    ///   `entity_authority_status(..) == Some(Granted)` and returns early
    ///   otherwise, so the client only reaches `mirror` while it may mutate.
    ///
    /// Nothing suspends between those checks and the call, so the predicate
    /// cannot drift -- unlike root 2's send path, where a whole tick elapses.
    /// The panic is therefore the *documented contract* for user code that
    /// mutates without authority, not a latent crash. These two tests pin that
    /// contract so a future caller added without a gate fails here, in naia's
    /// own suite, rather than downstream.
    #[test]
    fn mirroring_without_the_right_to_mutate_is_a_loud_contract_violation() {
        for status in [
            EntityAuthStatus::Available,
            EntityAuthStatus::Releasing,
            EntityAuthStatus::Denied,
        ] {
            let mut property = property_at(HostType::Client, status);
            assert!(
                !property.can_mutate(),
                "{status:?} must be a non-mutable client status for this test",
            );
            let message = panic_message_of(|| property.mirror(&"next".to_string()));
            assert!(
                message
                    .as_deref()
                    .is_some_and(|m| m.contains("Must request authority to mutate")),
                "mirror at {status:?} must panic with the authority contract \
                 message, got {message:?}",
            );
        }
    }

    #[test]
    fn mirroring_is_allowed_wherever_the_client_may_mutate() {
        for status in [EntityAuthStatus::Requested, EntityAuthStatus::Granted] {
            let mut property = property_at(HostType::Client, status);
            assert!(property.can_mutate());
            property.mirror(&"next".to_string());
            assert_eq!(
                property.inner, "next",
                "mirror must apply the value at {status:?}",
            );
        }
    }

    /// The client half of the authority table, pinned as a whole. The send
    /// guard, the `write` panic and the `mutate` panic all read these three
    /// predicates; if a row moves, every audit conclusion above is void.
    #[test]
    fn the_client_authority_table_is_what_the_audit_assumed() {
        // (status, can_read, can_mutate, can_write)
        let table = [
            (EntityAuthStatus::Available, true, false, false),
            (EntityAuthStatus::Requested, false, true, false),
            (EntityAuthStatus::Granted, false, true, true),
            (EntityAuthStatus::Releasing, true, false, true),
            (EntityAuthStatus::Denied, true, false, false),
        ];
        for (status, can_read, can_mutate, can_write) in table {
            let property = property_at(HostType::Client, status);
            assert_eq!(property.can_read(), can_read, "can_read at {status:?}");
            assert_eq!(
                property.can_mutate(),
                can_mutate,
                "can_mutate at {status:?}"
            );
            assert_eq!(property.can_write(), can_write, "can_write at {status:?}");
        }
    }
}
