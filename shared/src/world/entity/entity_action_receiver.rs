use std::{
    collections::{HashMap, VecDeque},
    hash::Hash,
    marker::PhantomData,
};

use crate::{messages::channels::receivers::reliable_receiver::ReliableReceiver, sequence_less_than, world::component::component_kinds::ComponentKind, EntityAction, MessageIndex as ActionIndex};

// keep E here! TODO: remove
pub struct EntityActionReceiver<E: Copy + Hash + Eq> {
    receiver: ReliableReceiver<EntityAction<E>>,
    entity_channels: HashMap<E, EntityChannel<E>>,
}

impl<E: Copy + Hash + Eq> EntityActionReceiver<E> {
    pub fn new() -> Self {
        Self {
            receiver: ReliableReceiver::new(),
            entity_channels: HashMap::default(),
        }
    }

    pub fn track_hosts_redundant_remote_entity(
        &mut self,
        entity: &E,
        component_kinds: &Vec<ComponentKind>,
    ) {
        let mut entity_channel = EntityChannel::new(*entity);
        entity_channel.spawned = true;
        for component_kind in component_kinds {
            entity_channel
                .components
                .insert(*component_kind, ComponentChannel::new(None));
        }
        self.entity_channels.insert(*entity, entity_channel);
    }

    pub fn untrack_hosts_redundant_remote_entity(&mut self, entity: &E) {
        self.entity_channels.remove(entity);
    }

    /// Buffer a read [`EntityAction`] so that it can be processed later
    pub fn buffer_action(&mut self, action_index: ActionIndex, action: EntityAction<E>) {
        self.receiver.buffer_message(action_index, action);
    }

    /// Read all buffered [`EntityAction`] inside the `receiver` and process them.
    ///
    /// Outputs the list of [`EntityAction`] that can be executed now, buffer the rest
    /// into each entity's [`EntityChannel`]
    pub fn receive_actions(&mut self) -> Vec<EntityAction<E>> {
        let mut outgoing_actions = Vec::new();
        let incoming_actions = self.receiver.receive_messages();
        for (action_index, action) in incoming_actions {
            if let Some(entity) = action.entity() {
                self.entity_channels
                    .entry(entity)
                    .or_insert_with(|| EntityChannel::new(entity));
                let entity_channel = self.entity_channels.get_mut(&entity).unwrap();
                entity_channel.receive_action(action_index, action, &mut outgoing_actions);
            }
        }

        // TODO: VERY IMPORTANT! You need to figure out how to remove EntityChannels after they've been despawned!
        // keep in mind that you need to keep around entity channels to be able to receive messages for them still
        // RIGHT NOW THIS IS LEAKING MEMORY!
        // a TTL for these Entity Channels after they've been despawned is probably the way to go

        outgoing_actions
    }
}

// Entity Channel

// keep E here! TODO: remove
struct EntityChannel<E: Copy + Hash + Eq> {
    entity: E,
    last_canonical_index: Option<ActionIndex>,
    spawned: bool,
    components: HashMap<ComponentKind, ComponentChannel<E>>,
    waiting_spawns: OrderedIds<Vec<ComponentKind>>,
    waiting_despawns: OrderedIds<()>,
}

impl<E: Copy + Hash + Eq> EntityChannel<E> {
    pub fn new(entity: E) -> Self {
        Self {
            entity,
            spawned: false,
            components: HashMap::new(),
            waiting_spawns: OrderedIds::new(),
            waiting_despawns: OrderedIds::new(),
            last_canonical_index: None,
        }
    }

    /// Process the provided [`EntityAction`]:
    ///
    /// * Checks that [`EntityAction`] can be executed now
    /// * If so, add it to `outgoing_actions`
    /// * Else, add it to internal "waiting" buffers so we can check when the [`EntityAction`]
    ///   can be executed
    ///
    /// ([`EntityAction`]s might not be executable now, for example is an InsertComponent
    ///  is processed before the corresponding entity has been spawned)
    pub fn receive_action(
        &mut self,
        incoming_action_index: ActionIndex,
        incoming_action: EntityAction<E>,
        outgoing_actions: &mut Vec<EntityAction<E>>,
    ) {
        match incoming_action {
            EntityAction::SpawnEntity(_, components) => {
                self.receive_spawn_entity_action(
                    incoming_action_index,
                    components,
                    outgoing_actions,
                );
            }
            EntityAction::DespawnEntity(_) => {
                self.receive_despawn_entity_action(incoming_action_index, outgoing_actions);
            }
            EntityAction::InsertComponent(_, component) => {
                self.receive_insert_component_action(
                    incoming_action_index,
                    component,
                    outgoing_actions,
                );
            }
            EntityAction::RemoveComponent(_, component) => {
                self.receive_remove_component_action(
                    incoming_action_index,
                    component,
                    outgoing_actions,
                );
            }
            EntityAction::Noop => {}
        }
    }

    /// Process the entity action.
    /// When the entity is actually spawned on the client, send back an ack event
    /// to the server.
    pub fn receive_spawn_entity_action(
        &mut self,
        action_index: ActionIndex,
        components: Vec<ComponentKind>,
        outgoing_actions: &mut Vec<EntityAction<E>>,
    ) {
        // this is the problem:
        // the point of the receiver is to de-dup a given event, like a Spawn Action here
        // we only only convert the NEWEST spawn packet into a SpawnAction
        // so the problem we're running into is that: Two Spawn Packets are sent, 1 with components A, B, and 1 with components A, B, C
        // action_index will be the same for both, however ...

        // do not process any spawn OLDER than last received spawn index / despawn index
        if let Some(last_index) = self.last_canonical_index {
            if sequence_less_than(action_index, last_index) {
                return;
            }
        }

        if !self.spawned {
            self.spawned = true;
            outgoing_actions.push(EntityAction::SpawnEntity(self.entity, components));

            // pop ALL waiting spawns, despawns, inserts, and removes OLDER than spawn_index
            self.receive_canonical(action_index);

            // process any waiting despawns
            if let Some((despawn_index, _)) = self.waiting_despawns.inner.pop_front() {
                self.receive_despawn_entity_action(despawn_index, outgoing_actions);
            } else {
                // process any waiting inserts
                let mut inserted_components = Vec::new();
                for (component, component_state) in &mut self.components {
                    if let Some(insert_index) = component_state.waiting_inserts.inner.pop_front() {
                        inserted_components.push((insert_index, *component));
                    }
                }

                for ((index, _), component) in inserted_components {
                    self.receive_insert_component_action(index, component, outgoing_actions);
                }
            }
        } else {
            // buffer spawn for later
            self.waiting_spawns.push_back(action_index, components);
        }
    }

    /// Process the entity despawn action
    /// When the entity has actually been despawned on the client, add an ack to the
    /// `outgoing_actions`
    pub fn receive_despawn_entity_action(
        &mut self,
        index: ActionIndex,
        outgoing_actions: &mut Vec<EntityAction<E>>,
    ) {
        // do not process any despawn OLDER than last received spawn index / despawn index
        if let Some(last_index) = self.last_canonical_index {
            if sequence_less_than(index, last_index) {
                return;
            }
        }

        if self.spawned {
            self.spawned = false;
            outgoing_actions.push(EntityAction::DespawnEntity(self.entity));

            // pop ALL waiting spawns, despawns, inserts, and removes OLDER than despawn_index
            self.receive_canonical(index);

            // set all component channels to 'inserted = false'
            for value in self.components.values_mut() {
                value.inserted = false;
            }

            // process any waiting spawns
            if let Some((spawn_index, components)) = self.waiting_spawns.inner.pop_front() {
                self.receive_spawn_entity_action(spawn_index, components, outgoing_actions);
            }
        } else {
            // buffer despawn for later
            self.waiting_despawns.push_back(index, ());
        }
    }

    pub fn receive_insert_component_action(
        &mut self,
        index: ActionIndex,
        component: ComponentKind,
        outgoing_actions: &mut Vec<EntityAction<E>>,
    ) {
        // do not process any insert OLDER than last received spawn index / despawn index
        if let Some(last_index) = self.last_canonical_index {
            if sequence_less_than(index, last_index) {
                return;
            }
        }

        if let std::collections::hash_map::Entry::Vacant(e) = self.components.entry(component) {
            e.insert(ComponentChannel::new(self.last_canonical_index));
        }
        let component_state = self.components.get_mut(&component).unwrap();

        // do not process any insert OLDER than last received insert / remove index for
        // this component
        if let Some(last_index) = component_state.last_canonical_index {
            if sequence_less_than(index, last_index) {
                return;
            }
        }

        if !component_state.inserted {
            component_state.inserted = true;
            outgoing_actions.push(EntityAction::InsertComponent(self.entity, component));

            // pop ALL waiting inserts, and removes OLDER than insert_index (in reference to
            // component)
            component_state.receive_canonical(index);

            // process any waiting removes
            if let Some((remove_index, _)) = component_state.waiting_removes.inner.pop_front() {
                self.receive_remove_component_action(remove_index, component, outgoing_actions);
            }
        } else {
            // buffer insert
            component_state.waiting_inserts.push_back(index, ());
        }
    }

    pub fn receive_remove_component_action(
        &mut self,
        index: ActionIndex,
        component: ComponentKind,
        outgoing_actions: &mut Vec<EntityAction<E>>,
    ) {
        // do not process any remove OLDER than last received spawn index / despawn index
        if let Some(last_index) = self.last_canonical_index {
            if sequence_less_than(index, last_index) {
                return;
            }
        }

        if let std::collections::hash_map::Entry::Vacant(e) = self.components.entry(component) {
            e.insert(ComponentChannel::new(self.last_canonical_index));
        }
        let component_state = self.components.get_mut(&component).unwrap();

        // do not process any remove OLDER than last received insert / remove index for
        // this component
        if let Some(last_index) = component_state.last_canonical_index {
            if sequence_less_than(index, last_index) {
                return;
            }
        }

        if component_state.inserted {
            component_state.inserted = false;
            outgoing_actions.push(EntityAction::RemoveComponent(self.entity, component));

            // pop ALL waiting inserts, and removes OLDER than remove_index (in reference to
            // component)
            component_state.receive_canonical(index);

            // process any waiting inserts
            if let Some((insert_index, _)) = component_state.waiting_inserts.inner.pop_front() {
                self.receive_insert_component_action(insert_index, component, outgoing_actions);
            }
        } else {
            // buffer remove
            component_state.waiting_removes.push_back(index, ());
        }
    }

    pub fn receive_canonical(&mut self, index: ActionIndex) {
        // pop ALL waiting spawns, despawns, inserts, and removes OLDER than index
        self.waiting_spawns.pop_front_until_and_including(index);
        self.waiting_despawns.pop_front_until_and_including(index);
        for component_state in self.components.values_mut() {
            component_state.receive_canonical(index);
        }

        self.last_canonical_index = Some(index);
    }
}

// Component Channel
// most of this should be public, no methods here

// keep E here! TODO: remove
pub struct ComponentChannel<E: Copy + Hash + Eq> {
    pub inserted: bool,
    pub last_canonical_index: Option<ActionIndex>,
    pub waiting_inserts: OrderedIds<()>,
    pub waiting_removes: OrderedIds<()>,

    phantom_e: PhantomData<E>,
}

impl<E: Copy + Hash + Eq> ComponentChannel<E> {
    pub fn new(canonical_index: Option<ActionIndex>) -> Self {
        Self {
            inserted: false,
            waiting_inserts: OrderedIds::new(),
            waiting_removes: OrderedIds::new(),
            last_canonical_index: canonical_index,

            phantom_e: PhantomData,
        }
    }

    pub fn receive_canonical(&mut self, index: ActionIndex) {
        // pop ALL waiting inserts, and removes OLDER than index
        self.waiting_inserts.pop_front_until_and_including(index);
        self.waiting_removes.pop_front_until_and_including(index);

        self.last_canonical_index = Some(index);
    }
}

pub struct OrderedIds<P> {
    // front small, back big
    inner: VecDeque<(ActionIndex, P)>,
}

impl<P> OrderedIds<P> {
    pub fn new() -> Self {
        Self {
            inner: VecDeque::new(),
        }
    }

    // pub fn push_front(&mut self, index: ActionIndex) {
    //     let mut index = 0;
    //
    //     loop {
    //         if index == self.inner.len() {
    //             self.inner.push_back(index);
    //             return;
    //         }
    //
    //         let old_index = self.inner.get(index).unwrap();
    //         if sequence_greater_than(*old_index, index) {
    //             self.inner.insert(index, index);
    //             return;
    //         }
    //
    //         index += 1
    //     }
    // }

    pub fn push_back(&mut self, action_index: ActionIndex, item: P) {
        let mut current_index = self.inner.len();

        loop {
            if current_index == 0 {
                self.inner.push_front((action_index, item));
                return;
            }

            current_index -= 1;

            let (old_index, _) = self.inner.get(current_index).unwrap();
            if sequence_less_than(*old_index, action_index) {
                self.inner.insert(current_index + 1, (action_index, item));
                return;
            }
        }
    }

    pub fn pop_front_until_and_including(&mut self, index: ActionIndex) {
        let mut pop = false;

        if let Some((old_index, _)) = self.inner.front() {
            if *old_index == index || sequence_less_than(*old_index, index) {
                pop = true;
            }
        }

        if pop {
            self.inner.pop_front();
        }
    }
}
