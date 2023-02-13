use bevy_ecs::{
    system::{SystemMeta, SystemParamFetch, SystemParamState},
    world::World,
};

use super::client::Client;

// State

pub struct State;

// SAFE: only local state is accessed
unsafe impl SystemParamState for State {
    fn init(_world: &mut World, _system_meta: &mut SystemMeta) -> Self {
        Self
    }
}

impl<'world, 'state> SystemParamFetch<'world, 'state> for State {
    type Item = Client<'world>;

    #[inline]
    unsafe fn get_param(
        _state: &'state mut Self,
        _system_meta: &SystemMeta,
        world: &'world World,
        _change_tick: u32,
    ) -> Self::Item {
        Client::new(world)
    }
}
