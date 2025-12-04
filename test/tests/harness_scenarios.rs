use naia_test::{
    harness::Scenario,
    protocol, Auth, Position,
};

/// Test: single client spawn replicates to server
#[test]
fn harness_single_client_spawn_replicates_to_server() {
    let mut scenario = Scenario::new(protocol());
    
    scenario.server_start();
    let client_a_key = scenario.client_connect("Client A", Auth::new("client_a", "password"));
    
    // Mutate phase: client spawns entity
    let entity_a = scenario.mutate(|ctx| {
        ctx.client(client_a_key, |client_a| {
            client_a.spawn(|mut spawned_entity| {
                spawned_entity
                    .insert_component(Position::new(1.0, 2.0));
            })
        })
    });
    
    // Expect phase: server has entity
    scenario.expect(|ctx| {
        ctx.server(|server| {
            server.has_entity(entity_a)
        })
    });
}

// /// Test: two clients see the same logical entity
// #[test]
// fn harness_two_clients_entity_mapping() {
//     let mut scenario = Scenario::new(protocol());
//     scenario.server_start();
//
//     let a = scenario.client_connect(Auth::new("client_a", "password"), "Client A");
//     let b = scenario.client_connect(Auth::new("client_b", "password"), "Client B");
//
//     // Mutate phase: client A spawns entity
//     let ent = scenario.mutate(|ctx| {
//         ctx.client(a, |c| {
//             c.spawn().with_position(Position::new(10.0, 20.0)).track()
//         })
//     });
//
//     // Wait for entity to replicate to server
//     scenario.expect(|ctx| {
//         ctx.server(|sv| {
//             sv.has_entity(ent)
//         })
//     });
//
//     // Now include B in scope
//     scenario.mutate(|ctx| {
//         ctx.server(|sv| {
//             sv.include_in_scope(b, ent);
//         });
//     });
//
//     // Expect phase: client B sees entity
//     scenario.expect(|ctx| {
//         ctx.client(b, |c| {
//             c.sees(ent)
//         })
//     });
//
//     // Additional expect: both clients report same position after A changes it
//     scenario.mutate(|ctx| {
//         ctx.client(a, |c| {
//             c.entity(ent).set_position(Position::new(100.0, 200.0));
//         });
//     });
//
//     scenario.expect(|ctx| {
//         ctx.client(a, |c| {
//             c.entity(ent).position_is(100.0, 200.0)
//         }) && ctx.client(b, |c| {
//             c.entity(ent).position_is(100.0, 200.0)
//         })
//     });
// }