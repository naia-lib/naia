use std::marker::PhantomData;

use bevy_ecs::{
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
    fn init(_world: &mut World, _system_meta: &mut SystemMeta) -> Self {
        Self {
            phantom_p: PhantomData,
            phantom_c: PhantomData,
        }
    }
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
