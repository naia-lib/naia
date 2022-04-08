use std::marker::PhantomData;

use bevy::ecs::{
    system::{SystemMeta, SystemParamFetch, SystemParamState},
    world::World,
};

use naia_client::shared::{ChannelIndex, Protocolize};

use super::client::Client;

// State

pub struct State<P: Protocolize, C: ChannelIndex> {
    phantom_p: PhantomData<P>,
    phantom_c: PhantomData<C>,
}

// SAFE: only local state is accessed
unsafe impl<P: Protocolize, C: ChannelIndex> SystemParamState for State<P, C> {
    type Config = ();

    fn init(_world: &mut World, _system_state: &mut SystemMeta, _config: Self::Config) -> Self {
        State {
            phantom_p: PhantomData,
            phantom_c: PhantomData,
        }
    }

    fn apply(&mut self, _world: &mut World) {}

    fn default_config() {}
}

impl<'world, 'state, P: Protocolize, C: ChannelIndex> SystemParamFetch<'world, 'state>
    for State<P, C>
{
    type Item = Client<'world, P, C>;

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
