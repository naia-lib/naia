use std::marker::PhantomData;

use bevy::ecs::{
    system::{SystemParamFetch, SystemParamState, SystemState},
    world::World,
};

use naia_client::ProtocolType;

use super::client::Client;

// State

pub struct State<P: ProtocolType> {
    phantom_p: PhantomData<P>,
}

// SAFE: only local state is accessed
unsafe impl<P: ProtocolType> SystemParamState for State<P> {
    type Config = ();

    fn init(_world: &mut World, _system_state: &mut SystemState, _config: Self::Config) -> Self {
        State {
            phantom_p: PhantomData,
        }
    }

    fn apply(&mut self, _world: &mut World) {}

    fn default_config() {}
}

impl<'a, P: ProtocolType> SystemParamFetch<'a> for State<P> {
    type Item = Client<'a, P>;

    #[inline]
    unsafe fn get_param(
        _state: &'a mut Self,
        _system_state: &'a SystemState,
        world: &'a World,
        _change_tick: u32,
    ) -> Self::Item {
        Client::new(world)
    }
}
