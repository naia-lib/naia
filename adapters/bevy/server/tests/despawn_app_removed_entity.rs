//! nydus-bnn: the app may remove a replicated entity from the bevy world before
//! naia's wire Despawn arrives (cyberlith's deterministic sim does this on the
//! confirmed timeline). The per-component lookups on that Despawn path must
//! treat the absent entity as already gone instead of panicking.

use naia_bevy_shared::bevy_ecs::world::World;
use naia_bevy_shared::{
    ComponentKind, Protocol as BevyProtocol, WorldData, WorldMut, WorldMutType,
};
use naia_test_harness::test_protocol::Position;

fn world_data() -> WorldData {
    let mut protocol = BevyProtocol::builder();
    protocol.register_component::<Position>();
    protocol.build().take_world_data()
}

#[test]
fn component_lookups_treat_an_app_removed_entity_as_gone() {
    let mut world = World::new();
    world.insert_resource(world_data());
    let entity = world.spawn(Position::new(1.0, 2.0)).id();
    assert!(world.despawn(entity));

    let mut proxy = WorldMut::new(&mut world);
    assert!(proxy.component_kinds(&entity).is_empty());
    assert!(proxy
        .remove_component_of_kind(&entity, ComponentKind::of::<Position>())
        .is_none());
}
