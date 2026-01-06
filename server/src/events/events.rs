use std::{collections::HashMap, hash::Hash};

use naia_shared::{
    Channel, ChannelKind, ComponentKind, GlobalResponseId, Message, MessageContainer, MessageKind,
    Replicate, Request,
};

use crate::{
    events::{
        main_events::{AuthEvent, ConnectEvent, ErrorEvent, MainEvent, MainEvents},
        world_events::{
            DelegateEntityEvent, DespawnEntityEvent, EntityAuthGrantEvent, EntityAuthResetEvent,
            InsertComponentEvent, MessageEvent, PublishEntityEvent, RemoveComponentEvent,
            RequestEvent, SpawnEntityEvent, UnpublishEntityEvent, UpdateComponentEvent, WorldEvent,
            WorldEvents,
        },
    },
    user::UserKey,
    DisconnectEvent,
};

pub struct Events<E: Hash + Copy + Eq + Sync + Send> {
    main_events: MainEvents,
    world_events: WorldEvents<E>,
}

impl<E: Hash + Copy + Eq + Sync + Send> From<WorldEvents<E>> for Events<E> {
    fn from(world_events: WorldEvents<E>) -> Self {
        Self::new(MainEvents::default(), world_events)
    }
}

impl<E: Hash + Copy + Eq + Sync + Send> Events<E> {
    pub(crate) fn new(mut main_events: MainEvents, mut world_events: WorldEvents<E>) -> Self {
        if main_events.has::<ConnectEvent>() {
            panic!("When using combined Main and World events, MainEvents should not contain ConnectEvent");
        }

        // combine error events
        if main_events.has::<ErrorEvent>() {
            for error in main_events.read::<ErrorEvent>() {
                world_events.push_error(error);
            }
        }

        Self {
            main_events,
            world_events,
        }
    }

    // Public

    pub fn is_empty(&self) -> bool {
        self.main_events.is_empty() && self.world_events.is_empty()
    }

    pub fn read<V: Event<E>>(&mut self) -> V::Iter {
        return V::iter(self);
    }

    pub fn has<V: Event<E>>(&self) -> bool {
        return V::has(self);
    }

    // This method is exposed for adapter crates ... prefer using Events.read::<SomeEvent>() instead.
    pub fn has_messages(&self) -> bool {
        self.world_events.has_messages()
    }
    pub fn take_messages(
        &mut self,
    ) -> HashMap<ChannelKind, HashMap<MessageKind, Vec<(UserKey, MessageContainer)>>> {
        self.world_events.take_messages()
    }

    // This method is exposed for adapter crates ... prefer using Events.read::<SomeEvent>() instead.
    pub fn has_requests(&self) -> bool {
        self.world_events.has_requests()
    }
    pub fn take_requests(
        &mut self,
    ) -> HashMap<
        ChannelKind,
        HashMap<MessageKind, Vec<(UserKey, GlobalResponseId, MessageContainer)>>,
    > {
        self.world_events.take_requests()
    }

    // This method is exposed for adapter crates ... prefer using Events.read::<SomeEvent>() instead.
    pub fn has_auths(&self) -> bool {
        self.main_events.has_auths()
    }
    pub fn take_auths(&mut self) -> HashMap<MessageKind, Vec<(UserKey, MessageContainer)>> {
        self.main_events.take_auths()
    }

    // These methods are exposed for adapter crates ... prefer using Events.read::<SomeEvent>() instead.
    pub fn has_inserts(&self) -> bool {
        self.world_events.has_inserts()
    }
    pub fn take_inserts(&mut self) -> Option<HashMap<ComponentKind, Vec<(UserKey, E)>>> {
        self.world_events.take_inserts()
    }

    // These methods are exposed for adapter crates ... prefer using Events.read::<SomeEvent>() instead.
    pub fn has_updates(&self) -> bool {
        self.world_events.has_updates()
    }
    pub fn take_updates(&mut self) -> Option<HashMap<ComponentKind, Vec<(UserKey, E)>>> {
        self.world_events.take_updates()
    }

    // These method are exposed for adapter crates ... prefer using Events.read::<SomeEvent>() instead.
    pub fn has_removes(&self) -> bool {
        self.world_events.has_removes()
    }
    pub fn take_removes(
        &mut self,
    ) -> Option<HashMap<ComponentKind, Vec<(UserKey, E, Box<dyn Replicate>)>>> {
        self.world_events.take_removes()
    }
}

// Event Trait
pub trait Event<E: Hash + Copy + Eq + Sync + Send> {
    type Iter;

    fn iter(events: &mut Events<E>) -> Self::Iter;

    fn has(events: &Events<E>) -> bool;
}

// Connect Event
impl<E: Hash + Copy + Eq + Sync + Send> Event<E> for ConnectEvent {
    type Iter = <ConnectEvent as WorldEvent<E>>::Iter;

    fn iter(events: &mut Events<E>) -> Self::Iter {
        <ConnectEvent as WorldEvent<E>>::iter(&mut events.world_events)
    }

    fn has(events: &Events<E>) -> bool {
        <ConnectEvent as WorldEvent<E>>::has(&events.world_events)
    }
}

// Disconnect Event
impl<E: Hash + Copy + Eq + Sync + Send> Event<E> for DisconnectEvent {
    type Iter = <DisconnectEvent as WorldEvent<E>>::Iter;

    fn iter(events: &mut Events<E>) -> Self::Iter {
        <DisconnectEvent as WorldEvent<E>>::iter(&mut events.world_events)
    }

    fn has(events: &Events<E>) -> bool {
        <DisconnectEvent as WorldEvent<E>>::has(&events.world_events)
    }
}

// Error Event
impl<E: Hash + Copy + Eq + Sync + Send> Event<E> for ErrorEvent {
    type Iter = <ErrorEvent as WorldEvent<E>>::Iter;

    fn iter(events: &mut Events<E>) -> Self::Iter {
        <ErrorEvent as WorldEvent<E>>::iter(&mut events.world_events)
    }

    fn has(events: &Events<E>) -> bool {
        <ErrorEvent as WorldEvent<E>>::has(&events.world_events)
    }
}

// Auth Event
impl<E: Hash + Copy + Eq + Sync + Send, M: Message> Event<E> for AuthEvent<M> {
    type Iter = <AuthEvent<M> as MainEvent>::Iter;

    fn iter(events: &mut Events<E>) -> Self::Iter {
        <AuthEvent<M> as MainEvent>::iter(&mut events.main_events)
    }

    fn has(events: &Events<E>) -> bool {
        <AuthEvent<M> as MainEvent>::has(&events.main_events)
    }
}

// Message Event
impl<E: Hash + Copy + Eq + Sync + Send, C: Channel, M: Message> Event<E> for MessageEvent<C, M> {
    type Iter = <MessageEvent<C, M> as WorldEvent<E>>::Iter;

    fn iter(events: &mut Events<E>) -> Self::Iter {
        <MessageEvent<C, M> as WorldEvent<E>>::iter(&mut events.world_events)
    }

    fn has(events: &Events<E>) -> bool {
        <MessageEvent<C, M> as WorldEvent<E>>::has(&events.world_events)
    }
}

// Request Event
impl<E: Hash + Copy + Eq + Sync + Send, C: Channel, Q: Request> Event<E> for RequestEvent<C, Q> {
    type Iter = <RequestEvent<C, Q> as WorldEvent<E>>::Iter;

    fn iter(events: &mut Events<E>) -> Self::Iter {
        <RequestEvent<C, Q> as WorldEvent<E>>::iter(&mut events.world_events)
    }

    fn has(events: &Events<E>) -> bool {
        <RequestEvent<C, Q> as WorldEvent<E>>::has(&events.world_events)
    }
}

// Spawn Entity Event
impl<E: Hash + Copy + Eq + Sync + Send> Event<E> for SpawnEntityEvent {
    type Iter = <SpawnEntityEvent as WorldEvent<E>>::Iter;

    fn iter(events: &mut Events<E>) -> Self::Iter {
        <SpawnEntityEvent as WorldEvent<E>>::iter(&mut events.world_events)
    }

    fn has(events: &Events<E>) -> bool {
        <SpawnEntityEvent as WorldEvent<E>>::has(&events.world_events)
    }
}

// Despawn Entity Event
impl<E: Hash + Copy + Eq + Sync + Send> Event<E> for DespawnEntityEvent {
    type Iter = <DespawnEntityEvent as WorldEvent<E>>::Iter;

    fn iter(events: &mut Events<E>) -> Self::Iter {
        <DespawnEntityEvent as WorldEvent<E>>::iter(&mut events.world_events)
    }

    fn has(events: &Events<E>) -> bool {
        <DespawnEntityEvent as WorldEvent<E>>::has(&events.world_events)
    }
}

// Publish Entity Event
impl<E: Hash + Copy + Eq + Sync + Send> Event<E> for PublishEntityEvent {
    type Iter = <PublishEntityEvent as WorldEvent<E>>::Iter;

    fn iter(events: &mut Events<E>) -> Self::Iter {
        <PublishEntityEvent as WorldEvent<E>>::iter(&mut events.world_events)
    }

    fn has(events: &Events<E>) -> bool {
        <PublishEntityEvent as WorldEvent<E>>::has(&events.world_events)
    }
}

// Unpublish Entity Event
impl<E: Hash + Copy + Eq + Sync + Send> Event<E> for UnpublishEntityEvent {
    type Iter = <UnpublishEntityEvent as WorldEvent<E>>::Iter;

    fn iter(events: &mut Events<E>) -> Self::Iter {
        <UnpublishEntityEvent as WorldEvent<E>>::iter(&mut events.world_events)
    }

    fn has(events: &Events<E>) -> bool {
        <UnpublishEntityEvent as WorldEvent<E>>::has(&events.world_events)
    }
}

// Delegate Entity Event
impl<E: Hash + Copy + Eq + Sync + Send> Event<E> for DelegateEntityEvent {
    type Iter = <DelegateEntityEvent as WorldEvent<E>>::Iter;

    fn iter(events: &mut Events<E>) -> Self::Iter {
        <DelegateEntityEvent as WorldEvent<E>>::iter(&mut events.world_events)
    }

    fn has(events: &Events<E>) -> bool {
        <DelegateEntityEvent as WorldEvent<E>>::has(&events.world_events)
    }
}

// Entity Auth Grant Event
impl<E: Hash + Copy + Eq + Sync + Send> Event<E> for EntityAuthGrantEvent {
    type Iter = <EntityAuthGrantEvent as WorldEvent<E>>::Iter;

    fn iter(events: &mut Events<E>) -> Self::Iter {
        <EntityAuthGrantEvent as WorldEvent<E>>::iter(&mut events.world_events)
    }

    fn has(events: &Events<E>) -> bool {
        <EntityAuthGrantEvent as WorldEvent<E>>::has(&events.world_events)
    }
}

// Entity Auth Reset Event
impl<E: Hash + Copy + Eq + Sync + Send> Event<E> for EntityAuthResetEvent {
    type Iter = <EntityAuthResetEvent as WorldEvent<E>>::Iter;

    fn iter(events: &mut Events<E>) -> Self::Iter {
        <EntityAuthResetEvent as WorldEvent<E>>::iter(&mut events.world_events)
    }

    fn has(events: &Events<E>) -> bool {
        <EntityAuthResetEvent as WorldEvent<E>>::has(&events.world_events)
    }
}

// Insert Component Event
impl<E: Hash + Copy + Eq + Sync + Send, C: Replicate> Event<E> for InsertComponentEvent<C> {
    type Iter = <InsertComponentEvent<C> as WorldEvent<E>>::Iter;

    fn iter(events: &mut Events<E>) -> Self::Iter {
        <InsertComponentEvent<C> as WorldEvent<E>>::iter(&mut events.world_events)
    }

    fn has(events: &Events<E>) -> bool {
        <InsertComponentEvent<C> as WorldEvent<E>>::has(&events.world_events)
    }
}

// Update Component Event
impl<E: Hash + Copy + Eq + Sync + Send, C: Replicate> Event<E> for UpdateComponentEvent<C> {
    type Iter = <UpdateComponentEvent<C> as WorldEvent<E>>::Iter;

    fn iter(events: &mut Events<E>) -> Self::Iter {
        <UpdateComponentEvent<C> as WorldEvent<E>>::iter(&mut events.world_events)
    }

    fn has(events: &Events<E>) -> bool {
        <UpdateComponentEvent<C> as WorldEvent<E>>::has(&events.world_events)
    }
}

// Remove Component Event
impl<E: Hash + Copy + Eq + Sync + Send, C: Replicate> Event<E> for RemoveComponentEvent<C> {
    type Iter = <RemoveComponentEvent<C> as WorldEvent<E>>::Iter;

    fn iter(events: &mut Events<E>) -> Self::Iter {
        <RemoveComponentEvent<C> as WorldEvent<E>>::iter(&mut events.world_events)
    }

    fn has(events: &Events<E>) -> bool {
        <RemoveComponentEvent<C> as WorldEvent<E>>::has(&events.world_events)
    }
}
