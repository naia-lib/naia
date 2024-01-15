use std::marker::PhantomData;

use bevy_ecs::component::Component;

#[derive(Component)]
pub struct HostOwned<T: Send + Sync + 'static> {
    phantom_t: PhantomData<T>,
}

impl<T: Send + Sync + 'static> HostOwned<T> {
    pub fn new() -> Self {
        Self {
            phantom_t: PhantomData,
        }
    }
}
