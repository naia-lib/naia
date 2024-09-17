use bevy_app::App;
use bevy_ecs::{
    change_detection::Mut,
    entity::Entity,
    event::{Event, EventReader, EventWriter},
    prelude::{Resource, World as BevyWorld},
    system::SystemState,
};

use crate::{
    events::{InsertComponentEvents, RemoveComponentEvents, UpdateComponentEvents},
    Replicate, Tick,
};

// ComponentEvent
pub enum ComponentEvents<T> {
    Insert(InsertComponentEvents<T>),
    Update(UpdateComponentEvents<T>),
    Remove(RemoveComponentEvents<T>),
}

impl<T: Send + Sync + 'static> ComponentEvents<T> {
    pub fn is_insert(&self) -> bool {
        match self {
            Self::Insert(_) => true,
            _ => false,
        }
    }

    pub fn as_insert(&self) -> &InsertComponentEvents<T> {
        match self {
            Self::Insert(events) => events,
            _ => panic!("ComponentEvents is not Insert"),
        }
    }

    pub fn process<C: Replicate>(&self, world: &mut BevyWorld) {
        match self {
            Self::Insert(events) => insert_component_event::<T, C>(world, &events),
            Self::Update(events) => update_component_event::<T, C>(world, &events),
            Self::Remove(events) => remove_component_event::<T, C>(world, &events),
        }
    }
}

#[derive(Event)]
pub struct InsertComponentEvent<T: Send + Sync + 'static, C: Replicate> {
    pub entity: Entity,
    phantom_t: std::marker::PhantomData<T>,
    phantom_c: std::marker::PhantomData<C>,
}

impl<T: Send + Sync + 'static, C: Replicate> InsertComponentEvent<T, C> {
    pub fn new(entity: Entity) -> Self {
        Self {
            entity,
            phantom_t: std::marker::PhantomData,
            phantom_c: std::marker::PhantomData,
        }
    }
}

#[derive(Event)]
pub struct UpdateComponentEvent<T: Send + Sync + 'static, C: Replicate> {
    pub tick: Tick,
    pub entity: Entity,
    phantom_t: std::marker::PhantomData<T>,
    phantom_c: std::marker::PhantomData<C>,
}

impl<T: Send + Sync + 'static, C: Replicate> UpdateComponentEvent<T, C> {
    pub fn new(tick: Tick, entity: Entity) -> Self {
        Self {
            tick,
            entity,
            phantom_t: std::marker::PhantomData,
            phantom_c: std::marker::PhantomData,
        }
    }
}

#[derive(Event)]
pub struct RemoveComponentEvent<T: Send + Sync + 'static, C: Replicate> {
    pub entity: Entity,
    phantom_t: std::marker::PhantomData<T>,
    pub component: C,
}

impl<T: Send + Sync + 'static, C: Replicate> RemoveComponentEvent<T, C> {
    pub fn new(entity: Entity, component: C) -> Self {
        Self {
            entity,
            phantom_t: std::marker::PhantomData,
            component,
        }
    }
}

// App Extension Methods
pub trait AppRegisterComponentEvents {
    fn add_component_events<T: Send + Sync + 'static, C: Replicate>(&mut self) -> &mut Self;
}

impl AppRegisterComponentEvents for App {
    fn add_component_events<T: Send + Sync + 'static, C: Replicate>(&mut self) -> &mut Self {
        self.add_event::<InsertComponentEvent<T, C>>()
            .add_event::<UpdateComponentEvent<T, C>>()
            .add_event::<RemoveComponentEvent<T, C>>();
        self
    }
}

// Startup State

#[derive(Resource)]
struct CachedInsertComponentEventsState<T: Send + Sync + 'static> {
    event_state: SystemState<EventReader<'static, 'static, InsertComponentEvents<T>>>,
}

#[derive(Resource)]
struct CachedUpdateComponentEventsState<T: Send + Sync + 'static> {
    event_state: SystemState<EventReader<'static, 'static, UpdateComponentEvents<T>>>,
}

#[derive(Resource)]
struct CachedRemoveComponentEventsState<T: Send + Sync + 'static> {
    event_state: SystemState<EventReader<'static, 'static, RemoveComponentEvents<T>>>,
}

// this is a system
pub fn component_events_startup<T: Send + Sync + 'static>(world: &mut BevyWorld) {
    let insert_event_state: SystemState<EventReader<InsertComponentEvents<T>>> =
        SystemState::new(world);
    world.insert_resource(CachedInsertComponentEventsState {
        event_state: insert_event_state,
    });

    let update_event_state: SystemState<EventReader<UpdateComponentEvents<T>>> =
        SystemState::new(world);
    world.insert_resource(CachedUpdateComponentEventsState {
        event_state: update_event_state,
    });

    let remove_event_state: SystemState<EventReader<RemoveComponentEvents<T>>> =
        SystemState::new(world);
    world.insert_resource(CachedRemoveComponentEventsState {
        event_state: remove_event_state,
    });
}

// this is not a system! It should be wrapped!
pub fn get_component_events<T: Clone + Send + Sync + 'static>(
    world: &mut BevyWorld,
) -> Vec<ComponentEvents<T>> {
    let mut events_collection: Vec<ComponentEvents<T>> = Vec::new();

    // Insert

    world.resource_scope(
        |world, mut events_reader_state: Mut<CachedInsertComponentEventsState<T>>| {
            let mut events_reader = events_reader_state.event_state.get_mut(world);

            for events in events_reader.read() {
                let events_clone: InsertComponentEvents<T> = Clone::clone(events);
                // info!("insert_component_events() events");
                events_collection.push(ComponentEvents::Insert(events_clone));
            }
        },
    );

    // Update

    world.resource_scope(
        |world, mut events_reader_state: Mut<CachedUpdateComponentEventsState<T>>| {
            let mut events_reader = events_reader_state.event_state.get_mut(world);

            for events in events_reader.read() {
                let events_clone: UpdateComponentEvents<T> = Clone::clone(events);

                events_collection.push(ComponentEvents::Update(events_clone));
            }
        },
    );

    // Remove

    world.resource_scope(
        |world, mut events_reader_state: Mut<CachedRemoveComponentEventsState<T>>| {
            let mut events_reader = events_reader_state.event_state.get_mut(world);

            for events in events_reader.read() {
                let events_clone: RemoveComponentEvents<T> = Clone::clone(events);
                events_collection.push(ComponentEvents::Remove(events_clone));
            }
        },
    );

    events_collection
}

fn insert_component_event<T: Send + Sync + 'static, C: Replicate>(
    world: &mut BevyWorld,
    events: &InsertComponentEvents<T>,
) {
    let mut system_state: SystemState<EventWriter<InsertComponentEvent<T, C>>> =
        SystemState::new(world);
    let mut event_writer = system_state.get_mut(world);

    for entity in events.read::<C>() {
        event_writer.send(InsertComponentEvent::<T, C>::new(entity));
    }
}

fn update_component_event<T: Send + Sync + 'static, C: Replicate>(
    world: &mut BevyWorld,
    events: &UpdateComponentEvents<T>,
) {
    let mut system_state: SystemState<EventWriter<UpdateComponentEvent<T, C>>> =
        SystemState::new(world);
    let mut event_writer = system_state.get_mut(world);

    for (tick, entity) in events.read::<C>() {
        event_writer.send(UpdateComponentEvent::<T, C>::new(tick, entity));
    }
}

fn remove_component_event<T: Send + Sync + 'static, C: Replicate>(
    world: &mut BevyWorld,
    events: &RemoveComponentEvents<T>,
) {
    let mut system_state: SystemState<EventWriter<RemoveComponentEvent<T, C>>> =
        SystemState::new(world);
    let mut event_writer = system_state.get_mut(world);

    for (entity, component) in events.read::<C>() {
        event_writer.send(RemoveComponentEvent::<T, C>::new(entity, component));
    }
}
